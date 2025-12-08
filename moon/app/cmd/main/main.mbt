///|
// B+ Tree Node Structure
enum BPlusNode[K, V] {
  Internal(Array[K], Array[BPlusNode[K, V]])
  Leaf(Array[(K, V)], Option[BPlusNode[K, V]])
}

///|
// B+ Tree structure
struct BPlusTree[K, V] {
  root : BPlusNode[K, V];
  order : Int;
  size : Int
}

///|
// Create an empty B+ tree
pub fn[K, V] create_bplustree(order~ : Int = 3) -> BPlusTree[K, V] {
  BPlusTree::{
    root: Leaf([], None),
    order: order,
    size: 0
  }
}

///|
// Helper: find insertion position in sorted array
fn[K : Compare, V] find_insert_pos(entries : Array[(K, V)], key : K) -> Int {
  let mut i = 0
  while i < entries.length() {
    let (k, _) = entries[i]
    if key.compare(k) < 0 {
      break
    }
    i = i + 1
  }
  i
}

///|
// Helper: insert element at index
fn[T] insert_at_index(arr : Array[T], idx : Int, value : T) -> Array[T] {
  let mut result = []
  let mut i = 0
  while i < arr.length() {
    if i == idx {
      result = result + [value]
    }
    result = result + [arr[i]]
    i = i + 1
  }
  if idx == arr.length() {
    result = result + [value]
  }
  result
}

///|
// Helper: slice array
fn[T] array_slice(arr : Array[T], start : Int, end : Int) -> Array[T] {
  let mut result = []
  let mut i = start
  while i < end && i < arr.length() {
    result = result + [arr[i]]
    i = i + 1
  }
  result
}

///|
// Insert into B+ tree
pub fn[K : Compare, V] insert(tree : BPlusTree[K, V], key : K, value : V) -> BPlusTree[K, V] {
  let (new_root, split) = insert_rec(tree.root, key, value, tree.order)
  match split {
    Some((promoted, right)) => {
      BPlusTree::{
        root: Internal([promoted], [new_root, right]),
        order: tree.order,
        size: tree.size + 1
      }
    }
    None => {
      BPlusTree::{
        root: new_root,
        order: tree.order,
        size: tree.size + 1
      }
    }
  }
}

fn[K : Compare, V] insert_rec(
  node : BPlusNode[K, V],
  key : K,
  value : V,
  order : Int
) -> (BPlusNode[K, V], Option[(K, BPlusNode[K, V])]) {
  match node {
    Leaf(entries, next) => {
      let pos = find_insert_pos(entries, key)
      let new_entries = insert_at_index(entries, pos, (key, value))
      if new_entries.length() > 2 * order - 1 {
        let mid = order
        let left_entries = array_slice(new_entries, 0, mid)
        let right_entries = array_slice(new_entries, mid, new_entries.length())
        let (first_key, _) = right_entries[0]
        let right_node = Leaf(right_entries, next)
        let left_node = Leaf(left_entries, Some(right_node))
        (left_node, Some((first_key, right_node)))
      } else {
        (Leaf(new_entries, next), None)
      }
    }
    Internal(keys, children) => {
      let mut idx = 0
      while idx < keys.length() {
        if key.compare(keys[idx]) < 0 {
          break
        }
        idx = idx + 1
      }
      let (updated_child, split_result) = insert_rec(children[idx], key, value, order)
      match split_result {
        Some((promoted_key, right_child)) => {
          let new_keys = insert_at_index(keys, idx, promoted_key)
          let new_children = insert_at_index(children, idx + 1, right_child)
          let updated_children = set_at_index(new_children, idx, updated_child)
          if new_keys.length() > 2 * order - 1 {
            let mid = order
            let left_keys = array_slice(new_keys, 0, mid)
            let right_keys = array_slice(new_keys, mid + 1, new_keys.length())
            let promoted = new_keys[mid]
            let left_children = array_slice(updated_children, 0, mid + 1)
            let right_children = array_slice(updated_children, mid + 1, updated_children.length())
            let left_node = Internal(left_keys, left_children)
            let right_node = Internal(right_keys, right_children)
            (left_node, Some((promoted, right_node)))
          } else {
            (Internal(new_keys, updated_children), None)
          }
        }
        None => {
          let updated_children = set_at_index(children, idx, updated_child)
          (Internal(keys, updated_children), None)
        }
      }
    }
  }
}

fn[T] set_at_index(arr : Array[T], idx : Int, value : T) -> Array[T] {
  let mut result = []
  let mut i = 0
  while i < arr.length() {
    if i == idx {
      result = result + [value]
    } else {
      result = result + [arr[i]]
    }
    i = i + 1
  }
  result
}

///|
// Search in B+ tree
pub fn[K : Compare + Eq, V] search(tree : BPlusTree[K, V], key : K) -> Option[V] {
  search_rec(tree.root, key)
}

fn[K : Compare + Eq, V] search_rec(node : BPlusNode[K, V], key : K) -> Option[V] {
  match node {
    Leaf(entries, _) => {
      let mut i = 0
      while i < entries.length() {
        let (k, v) = entries[i]
        if k == key {
          return Some(v)
        }
        i = i + 1
      }
      None
    }
    Internal(keys, children) => {
      let mut idx = 0
      while idx < keys.length() {
        if key.compare(keys[idx]) < 0 {
          break
        }
        idx = idx + 1
      }
      search_rec(children[idx], key)
    }
  }
}

///|
// Range query
pub fn[K : Compare, V] range_query(tree : BPlusTree[K, V], start : K, end : K) -> Array[V] {
  let start_leaf = find_leaf(tree.root, start)
  collect_range(start_leaf, start, end, [])
}

fn[K : Compare, V] find_leaf(node : BPlusNode[K, V], key : K) -> BPlusNode[K, V] {
  match node {
    Leaf(_, _) => node
    Internal(keys, children) => {
      let mut idx = 0
      while idx < keys.length() {
        if key.compare(keys[idx]) < 0 {
          break
        }
        idx = idx + 1
      }
      find_leaf(children[idx], key)
    }
  }
}

fn[K : Compare, V] collect_range(
  node : BPlusNode[K, V],
  start : K,
  end : K,
  acc : Array[V]
) -> Array[V] {
  match node {
    Leaf(entries, next) => {
      let mut result = acc
      let mut i = 0
      while i < entries.length() {
        let (k, v) = entries[i]
        if k.compare(start) >= 0 && k.compare(end) <= 0 {
          result = result + [v]
        }
        i = i + 1
      }
      match next {
        Some(next_node) => {
          if entries.length() > 0 {
            let (last_key, _) = entries[entries.length() - 1]
            if last_key.compare(end) <= 0 {
              collect_range(next_node, start, end, result)
            } else {
              result
            }
          } else {
            result
          }
        }
        None => result
      }
    }
    _ => acc
  }
}

///|
// Traverse all entries in order
pub fn[K, V] traverse(tree : BPlusTree[K, V], f : (K, V) -> Unit) -> Unit {
  traverse_from_leaf(get_first_leaf(tree.root), f)
}

fn[K, V] get_first_leaf(node : BPlusNode[K, V]) -> BPlusNode[K, V] {
  match node {
    Leaf(_, _) => node
    Internal(_, children) => get_first_leaf(children[0])
  }
}

fn[K, V] traverse_from_leaf(node : BPlusNode[K, V], f : (K, V) -> Unit) -> Unit {
  match node {
    Leaf(entries, next) => {
      let mut i = 0
      while i < entries.length() {
        let (k, v) = entries[i]
        f(k, v)
        i = i + 1
      }
      match next {
        Some(next_node) => traverse_from_leaf(next_node, f)
        None => ()
      }
    }
    _ => ()
  }
}

///|
// Print tree (simplified)
pub fn[K : Show, V : Show] print_tree(tree : BPlusTree[K, V]) -> Unit {
  println("B+ Tree (order=" + tree.order.to_string() + ", size=" + tree.size.to_string() + ")")
  print_node(tree.root, 0)
}

fn[K : Show, V : Show] print_node(node : BPlusNode[K, V], depth : Int) -> Unit {
  let indent = repeat_string("  ", depth)
  match node {
    Leaf(entries, _) => {
      println(indent + "Leaf:")
      let mut i = 0
      while i < entries.length() {
        let (k, v) = entries[i]
        println(indent + "  " + k.to_string() + " -> " + v.to_string())
        i = i + 1
      }
    }
    Internal(keys, children) => {
      println(indent + "Internal keys: " + keys_to_string(keys))
      let mut i = 0
      while i < children.length() {
        print_node(children[i], depth + 1)
        i = i + 1
      }
    }
  }
}

fn[K : Show] keys_to_string(keys : Array[K]) -> String {
  let mut result = ""
  let mut i = 0
  while i < keys.length() {
    if i > 0 {
      result = result + ", "
    }
    result = result + keys[i].to_string()
    i = i + 1
  }
  result
}

fn repeat_string(s : String, n : Int) -> String {
  let mut result = ""
  let mut i = 0
  while i < n {
    result = result + s
    i = i + 1
  }
  result
}

///|
fn main {
  println(@lib.fib(10))
  
  // Create and test B+ tree
  let mut tree = create_bplustree(order = 3)
  
  // Insert values
  tree = insert(tree, 10, "ten")
  tree = insert(tree, 20, "twenty")
  tree = insert(tree, 5, "five")
  tree = insert(tree, 15, "fifteen")
  tree = insert(tree, 25, "twenty-five")
  tree = insert(tree, 30, "thirty")
  tree = insert(tree, 7, "seven")
  tree = insert(tree, 12, "twelve")
  tree = insert(tree, 18, "eighteen")
  tree = insert(tree, 22, "twenty-two")
  
  println("\n=== B+ Tree Structure ===")
  print_tree(tree)
  
  println("\n=== Search Operations ===")
  match search(tree, 15) {
    Some(v) => println("Found 15: " + v)
    None => println("15 not found")
  }
  match search(tree, 99) {
    Some(v) => println("Found 99: " + v)
    None => println("99 not found")
  }
  
  println("\n=== Range Query [10, 20] ===")
  let results = range_query(tree, 10, 20)
  println("Found " + results.length().to_string() + " results")
  
  println("\n=== In-order Traversal ===")
  traverse(tree, fn(k, v) {
    println("  " + k.to_string() + " -> " + v)
  })
}
