#include <iostream>
#include <memory>

struct Order {
    std::string name;

    Order(std::string name) : name(std::move(name)) {};
};

struct Holder {
    std::shared_ptr<Order> order;
};

int main(int argc, char *argv[]) {
    std::shared_ptr<Order> order = std::make_shared<Order>("hello");
    {
        auto order2 = order;

        char holderData[sizeof(Holder)];
        auto holder = new(&holderData) Holder{order};

        {
            // std::reinterpret_pointer_cast<std::shared_ptr<Order>>(holderData);
            // auto holderHydrate = (Holder)(holderData[0]);
            // auto holderHydrate2 = reinterpret_cast<std::shared_ptr<Order>&>(holderData);
            reinterpret_cast<Holder*>(holderData)->~Holder();
        }
    }
    std::cout << "sizeof(shared_ptr) " << sizeof(Holder) << std::endl;
    std::cout << order.use_count() << std::endl;
}