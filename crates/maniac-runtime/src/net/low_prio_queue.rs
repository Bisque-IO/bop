/// Low-Priority Queue - Prevents CPU-intensive operations from starving regular traffic
///
/// This is used primarily for SSL/TLS handshakes, which are CPU-intensive.
/// By limiting how many low-priority sockets we process per iteration,
/// we ensure regular traffic continues to flow smoothly.

use std::collections::VecDeque;
use mio::Token;
use crate::net::constants::MAX_LOW_PRIO_PER_ITERATION;

/// Low-priority socket queue
pub struct LowPriorityQueue {
    /// FIFO queue of low-priority sockets
    queue: VecDeque<Token>,

    /// How many low-priority sockets to process per iteration
    budget_per_iteration: usize,

    /// Remaining budget for current iteration
    budget_remaining: usize,
}

impl LowPriorityQueue {
    /// Create a new low-priority queue
    ///
    /// # Arguments
    /// * `budget` - Max number of low-priority sockets to process per iteration (default: 5)
    pub fn new(budget: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            budget_per_iteration: budget,
            budget_remaining: budget,
        }
    }

    /// Create with default budget
    pub fn with_default_budget() -> Self {
        Self::new(MAX_LOW_PRIO_PER_ITERATION)
    }

    /// Add a socket to the low-priority queue
    ///
    /// The socket will be processed in a future iteration when budget is available.
    pub fn push(&mut self, token: Token) {
        self.queue.push_back(token);
    }

    /// Check if there's budget remaining this iteration
    pub fn has_budget(&self) -> bool {
        self.budget_remaining > 0
    }

    /// Consume one unit of budget
    ///
    /// Should be called when processing a low-priority socket.
    pub fn consume_budget(&mut self) {
        if self.budget_remaining > 0 {
            self.budget_remaining -= 1;
        }
    }

    /// Reset budget for new iteration
    ///
    /// Should be called at the start of each event loop iteration.
    pub fn reset_budget(&mut self) {
        self.budget_remaining = self.budget_per_iteration;
    }

    /// Pop a socket from the queue if budget is available
    ///
    /// Returns None if queue is empty or no budget remains.
    pub fn pop(&mut self) -> Option<Token> {
        if self.has_budget() && !self.queue.is_empty() {
            self.consume_budget();
            self.queue.pop_front()
        } else {
            None
        }
    }

    /// Process sockets from the queue while budget remains
    ///
    /// Calls the provided callback for each socket, up to the budget limit.
    ///
    /// # Arguments
    /// * `callback` - Called for each low-priority socket to be processed
    ///
    /// # Returns
    /// Number of sockets processed
    pub fn process<F>(&mut self, mut callback: F) -> usize
    where
        F: FnMut(Token),
    {
        let mut processed = 0;

        while let Some(token) = self.pop() {
            callback(token);
            processed += 1;
        }

        processed
    }

    /// Get the number of sockets in the queue
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Get remaining budget for this iteration
    pub fn remaining_budget(&self) -> usize {
        self.budget_remaining
    }

    /// Set the budget for future iterations
    pub fn set_budget(&mut self, budget: usize) {
        self.budget_per_iteration = budget;
    }

    /// Remove a specific token from the queue
    ///
    /// Used when a socket is closed before it could be processed.
    pub fn remove(&mut self, token: Token) {
        self.queue.retain(|&t| t != token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_budget() {
        let mut queue = LowPriorityQueue::new(5);

        // Add 10 sockets
        for i in 0..10 {
            queue.push(Token(i));
        }

        assert_eq!(queue.len(), 10);

        // Process - should only process 5 due to budget
        let mut processed = Vec::new();
        queue.process(|token| processed.push(token));

        assert_eq!(processed.len(), 5);
        assert_eq!(queue.len(), 5);
        assert_eq!(queue.remaining_budget(), 0);
    }

    #[test]
    fn test_budget_reset() {
        let mut queue = LowPriorityQueue::new(3);

        for i in 0..10 {
            queue.push(Token(i));
        }

        // First iteration - process 3
        queue.process(|_| {});
        assert_eq!(queue.len(), 7);

        // Reset budget
        queue.reset_budget();
        assert_eq!(queue.remaining_budget(), 3);

        // Second iteration - process 3 more
        queue.process(|_| {});
        assert_eq!(queue.len(), 4);
    }

    #[test]
    fn test_fifo_order() {
        let mut queue = LowPriorityQueue::new(10);

        for i in 0..5 {
            queue.push(Token(i));
        }

        let mut processed = Vec::new();
        queue.process(|token| processed.push(token.0));

        // Should be in FIFO order
        assert_eq!(processed, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_manual_pop() {
        let mut queue = LowPriorityQueue::new(2);

        queue.push(Token(1));
        queue.push(Token(2));
        queue.push(Token(3));

        assert_eq!(queue.pop(), Some(Token(1)));
        assert_eq!(queue.pop(), Some(Token(2)));
        assert_eq!(queue.pop(), None); // Budget exhausted
        assert_eq!(queue.remaining_budget(), 0);
    }

    #[test]
    fn test_remove() {
        let mut queue = LowPriorityQueue::new(10);

        for i in 0..5 {
            queue.push(Token(i));
        }

        queue.remove(Token(2));
        assert_eq!(queue.len(), 4);

        let mut processed = Vec::new();
        queue.process(|token| processed.push(token.0));

        assert_eq!(processed, vec![0, 1, 3, 4]); // 2 is missing
    }

    #[test]
    fn test_consume_budget_directly() {
        let mut queue = LowPriorityQueue::new(3);

        assert_eq!(queue.remaining_budget(), 3);
        assert!(queue.has_budget());

        queue.consume_budget();
        assert_eq!(queue.remaining_budget(), 2);

        queue.consume_budget();
        queue.consume_budget();
        assert_eq!(queue.remaining_budget(), 0);
        assert!(!queue.has_budget());

        // Consuming beyond budget shouldn't panic or go negative
        queue.consume_budget();
        assert_eq!(queue.remaining_budget(), 0);
    }

    #[test]
    fn test_change_budget() {
        let mut queue = LowPriorityQueue::new(5);

        for i in 0..10 {
            queue.push(Token(i));
        }

        queue.process(|_| {});
        assert_eq!(queue.len(), 5);

        // Change budget and reset
        queue.set_budget(2);
        queue.reset_budget();

        queue.process(|_| {});
        assert_eq!(queue.len(), 3); // Only 2 more processed
    }
}
