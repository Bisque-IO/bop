package concurrent

import bop "../libbop"

MPMC :: bop.MPMC

MPMC_Producer_Token :: bop.MPMC_Producer_Token

MPMC_Consumer_Token :: bop.MPMC_Consumer_Token

MPMC_Blocking :: bop.MPMC_Blocking

MPMC_Blocking_Producer_Token :: bop.MPMC_Blocking_Producer_Token

MPMC_Blocking_Consumer_Token :: bop.MPMC_Blocking_Consumer_Token

mpmc_create :: bop.mpmc_create
mpmc_destroy :: bop.mpmc_destroy
mpmc_create_producer_token :: bop.mpmc_create_producer_token
mpmc_destroy_producer_token :: bop.mpmc_destroy_producer_token
mpmc_create_consumer_token :: bop.mpmc_create_consumer_token
mpmc_destroy_consumer_token :: bop.mpmc_destroy_consumer_token
mpmc_size_approx :: bop.mpmc_size_approx
mpmc_enqueue :: bop.mpmc_enqueue
mpmc_enqueue_token :: bop.mpmc_enqueue_token
mpmc_enqueue_bulk :: bop.mpmc_enqueue_bulk
mpmc_enqueue_bulk_token :: bop.mpmc_enqueue_bulk_token
mpmc_try_enqueue_bulk :: bop.mpmc_try_enqueue_bulk
mpmc_try_enqueue_bulk_token :: bop.mpmc_try_enqueue_bulk_token
mpmc_dequeue :: bop.mpmc_dequeue
mpmc_dequeue_token :: bop.mpmc_dequeue_token
mpmc_dequeue_bulk :: bop.mpmc_dequeue_bulk
mpmc_dequeue_bulk_token :: bop.mpmc_dequeue_bulk_token

mpmc_blocking_create :: bop.mpmc_blocking_create
mpmc_blocking_destroy :: bop.mpmc_blocking_destroy
mpmc_blocking_create_producer_token :: bop.mpmc_blocking_create_producer_token
mpmc_blocking_destroy_producer_token :: bop.mpmc_blocking_destroy_producer_token
mpmc_blocking_create_consumer_token :: bop.mpmc_blocking_create_consumer_token
mpmc_blocking_destroy_consumer_token :: bop.mpmc_blocking_destroy_consumer_token
mpmc_blocking_size_approx :: bop.mpmc_blocking_size_approx
mpmc_blocking_enqueue :: bop.mpmc_blocking_enqueue
mpmc_blocking_enqueue_token :: bop.mpmc_blocking_enqueue_token
mpmc_blocking_enqueue_bulk :: bop.mpmc_blocking_enqueue_bulk
mpmc_blocking_enqueue_bulk_token :: bop.mpmc_blocking_enqueue_bulk_token
mpmc_blocking_try_enqueue_bulk :: bop.mpmc_blocking_try_enqueue_bulk
mpmc_blocking_try_enqueue_bulk_token :: bop.mpmc_blocking_try_enqueue_bulk_token
mpmc_blocking_dequeue :: bop.mpmc_blocking_dequeue
mpmc_blocking_dequeue_token :: bop.mpmc_blocking_dequeue_token
mpmc_blocking_dequeue_wait :: bop.mpmc_blocking_dequeue_wait
mpmc_blocking_dequeue_wait_token :: bop.mpmc_blocking_dequeue_wait_token
mpmc_blocking_dequeue_bulk :: bop.mpmc_blocking_dequeue_bulk
mpmc_blocking_dequeue_bulk_token :: bop.mpmc_blocking_dequeue_bulk_token
mpmc_blocking_dequeue_bulk_wait :: bop.mpmc_blocking_dequeue_bulk_wait
mpmc_blocking_dequeue_bulk_wait_token :: bop.mpmc_blocking_dequeue_bulk_wait_token
