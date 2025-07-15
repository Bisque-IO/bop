#include "../src/mpmc.h"

#include <iostream>
#include <ostream>

int main(int argc, char **argv) {
    auto q = bop_mpmc_create();
    bop_mpmc_enqueue(q, 99);
    uint64_t items[128];
    auto size = bop_mpmc_dequeue_bulk(q, items, 128);
    std::cout << "size: " << size << "  item: " << items[0] << std::endl;
    return 0;
}
