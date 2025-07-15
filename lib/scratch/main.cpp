#include <cstring>
#include <iostream>
#include <memory>

struct Log_Entry {
    std::string name;

    Log_Entry(std::string name) : name(std::move(name)) {};
};

struct Holder {
    std::shared_ptr<Log_Entry> order;
};

const size_t BOP_RAFT_LOG_ENTRY_PTR_SIZE = sizeof(std::shared_ptr<Log_Entry>);

struct bop_raft_log_entry_ptr_holder {
    char data[BOP_RAFT_LOG_ENTRY_PTR_SIZE];
};

// struct bop_raft_log_entry_ptr {
//     std::shared_ptr<Log_Entry> holder;
//
//     explicit bop_raft_log_entry_ptr(const std::shared_ptr<Log_Entry> &holder)
//         : holder(holder) {
//     }
// };

size_t bop_raft_log_entry_ptr_use_count(bop_raft_log_entry_ptr_holder *log_entry_ptr) {
    return reinterpret_cast<std::shared_ptr<Log_Entry>*>(log_entry_ptr)->use_count();
}

void bop_raft_log_entry_ptr_retain(bop_raft_log_entry_ptr_holder* log_entry_ptr) {
    char data[BOP_RAFT_LOG_ENTRY_PTR_SIZE];
    new(data) std::shared_ptr(*reinterpret_cast<std::shared_ptr<Log_Entry>*>(log_entry_ptr));
}

void bop_raft_log_entry_ptr_release(bop_raft_log_entry_ptr_holder *log_entry_ptr) {
    reinterpret_cast<std::shared_ptr<Log_Entry>*>(log_entry_ptr)->~shared_ptr();
}

int main(int argc, char *argv[]) {
    std::shared_ptr<Log_Entry> order = std::make_shared<Log_Entry>("hello");
    bop_raft_log_entry_ptr_holder holder{};

    std::memcpy(holder.data, reinterpret_cast<char*>(&order), BOP_RAFT_LOG_ENTRY_PTR_SIZE);

    std::cout << order.use_count() << std::endl;
    {
        // auto order2 = order;
        // std::cout << order.use_count() << std::endl;

        // bop_raft_log_entry_ptr holderData{nullptr};
        bop_raft_log_entry_ptr_retain(&holder);
        std::cout << "after retain: " << order.use_count() << std::endl;

        // char holderData0[sizeof(Holder)];
        // auto holder = new(&holderData0) Holder{order};

        bop_raft_log_entry_ptr_release(&holder);
        std::cout << "after release: " << order.use_count() << std::endl;

        {
            // std::reinterpret_pointer_cast<std::shared_ptr<Order>>(holderData);
            // auto holderHydrate = (Holder)(holderData[0]);
            // auto holderHydrate2 = reinterpret_cast<std::shared_ptr<Order>&>(holderData);
            // reinterpret_cast<Holder*>(holderData)->~Holder();
        }
    }
    std::cout << "sizeof(shared_ptr) " << sizeof(Holder) << std::endl;
    std::cout << order.use_count() << std::endl;
}