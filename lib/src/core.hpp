#pragma once

// #include <boost/config.hpp>
// #include <boost/assert.hpp>
// #include <boost/system.hpp>
// #include <boost/core/ignore_unused.hpp>

// #include <fmt/format.h>
// #include <spdlog/spdlog.h>

// #include <algorithm>
// #include <array>
// #include <atomic>
// #include <charconv>
// #include <chrono>
// #include <cmath>
// #include <cstddef>
// #include <cstring>
// #include <exception>
// #include <expected>
// #include <filesystem>
// #include <iostream>
// #include <limits>
// #include <map>
#include <bit>
#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <expected>
#include <memory>
#include <queue>
#include <string>
#include <string_view>

// #include <mutex>
// #include <optional>
// #include <ostream>
// #include <random>
// #include <source_location>
// #include <stdexcept>
// #include <string>
// #include <string_view>
// #include <system_error>
// #include <thread>
// #include <tuple>
// #include <unordered_map>
// #include <utility>
// #include <vector>

// #include <boost/asio.hpp>

// #include "./detail/expected.hpp"

// #include "./thread.hpp"

// #include "asio/detail/socket_option.hpp"
// #include "ylt/util/time_util.h"

// #include <ostream>

// Macro to use in place of 'inline' to force a function to be inline
#if !defined(BQ_INLINE)
#  if defined(_MSC_VER)
#    define BQ_INLINE __forceinline
#  elif defined(__GNUC__) && __GNUC__ > 3
     // Clang also defines __GNUC__ (as 4)
#    define BQ_INLINE inline __attribute__ ((__always_inline__))
#  else
#    define BQ_INLINE inline
#  endif
#endif

template <typename F>
struct privDefer {
    F f;

    explicit privDefer(F f) : f(f) {}

    ~privDefer() {
        f();
    }
};

template <typename F>
privDefer<F> defer_func(F f) {
    return privDefer<F>(f);
}

#define DEFER_1(x, y) x##y
#define DEFER_2(x, y) DEFER_1(x, y)
#define DEFER_3(x) DEFER_2(x, __COUNTER__)
#define DEFER(code) auto DEFER_3(_defer_) = defer_func([&]() { code; })

#if _WIN32
#define INLINE inline
#define FORCE_INLINE
#else
#define INLINE __attribute__((always_inline))
#define FORCE_INLINE INLINE inline
#endif

// #if (defined(_MSC_VER) && _MSVC_LANG>= 202002L)
// #define LIKELY(x) (x) [[likely]]
// #else
// #define LIKELY(x) (__builtin_expect(!!(x), 1))
// #endif
//
// #if (defined(_MSC_VER) && _MSVC_LANG>= 202002L)// || __cplusplus >= 202002L
// #define UNLIKELY(x) (x) [[unlikely]]
// #else
// #define UNLIKELY(x) (__builtin_expect(!!(x), 0))
// #endif

#if defined(__x86_64__)
#define X86_64
#endif
#if defined(__i386__)
#define p_i386
#endif
#if defined(__aarch64__)
#define ARM64
#endif
#if defined(__arm__)
#define ARM
#endif

#define REMOVE_COPY_CAPABILITY(clazz)                                                              \
  private:                                                                                         \
    clazz(const clazz&) = delete;                                                                  \
    clazz& operator=(const clazz&) = delete;

#define REMOVE_MOVE_CAPABILITY(clazz)                                                              \
  private:                                                                                         \
    clazz(const clazz&&) = delete;                                                                 \
    auto operator=(const clazz&&) = delete;

#define REMOVE_COPY_AND_MOVE_CAPABILITY(clazz)                                                     \
  private:                                                                                         \
    clazz(const clazz&) = delete;                                                                  \
    clazz& operator=(const clazz&) = delete;                                                       \
    clazz(const clazz&&) = delete;                                                                 \
    auto operator=(const clazz&&) = delete;

namespace bop {
template <typename T>
using ptr = std::shared_ptr<T>;

/// weak_ptr
template <typename T>
using wptr = std::weak_ptr<T>;

template <typename T, typename... TArgs>
BQ_INLINE ptr<T> make_ptr(TArgs&&... args) {
    return std::make_shared<T>(std::forward<TArgs>(args)...);
    // return std::allocate_shared<T, mi_stl_allocator<T>>(
    //   mi_stl_allocator<T>{}, std::forward<TArgs>(args)...
    // );
}

template <typename T, std::endian ENDIAN>
struct alignas(sizeof(T)) endian_num {
    T get() const {
        if constexpr (std::endian::native == ENDIAN) {
            return value;
        } else {
            if constexpr (std::is_same<T, std::double_t>()) {
                int64_t r = std::byteswap(*reinterpret_cast<const int64_t*>(&value));
                return *reinterpret_cast<std::double_t*>(&r);
            } else if constexpr (std::is_same<T, std::float_t>()) {
                int32_t r = std::byteswap(*reinterpret_cast<const int32_t*>(&value));
                return *reinterpret_cast<std::float_t*>(&r);
            } else {
                return std::byteswap(value);
            }
        }
    }

    T set(T v) {
        if constexpr (std::endian::native == ENDIAN) {
            return value = v;
        } else {
            if constexpr (std::is_same<T, std::double_t>()) {
                int64_t r = std::byteswap(*reinterpret_cast<const int64_t*>(&v));
                return value = *reinterpret_cast<std::double_t*>(&r);
            } else if constexpr (std::is_same<T, std::float_t>()) {
                int32_t r = std::byteswap(*reinterpret_cast<const int32_t*>(&v));
                return value = *reinterpret_cast<std::float_t*>(&r);
            } else {
                return value = std::byteswap(v);
            }
        }
    }

    T get_raw() {
        return value;
    }

    T set_raw(T v) {
        return value = v;
    }

    std::string_view as_view() {
        return std::string_view{reinterpret_cast<const char*>(&value), sizeof(T)};
    }

    std::string& append_to(std::string& dst) {
        dst.append(as_view());
        return dst;
    }

    std::string& append_text_to(std::string& dst) {
        if constexpr (std::is_same<T, std::double_t>()) {
            char chars[256];
            if (auto result = std::to_chars(chars, chars + 256, get()); result.ec == std::errc{}) {
                dst.append(chars, result.ptr);
            }
            return dst;
        } else if constexpr (std::is_same<T, std::float_t>()) {
            char chars[128];
            if (auto result = std::to_chars(chars, chars + 128, get()); result.ec == std::errc{}) {
                dst.append(chars, result.ptr);
            }
            return dst;
        } else {
            char chars[24];
            if (auto result = std::to_chars(chars, chars + 24, get()); result.ec == std::errc{}) {
                dst.append(chars, result.ptr);
            }
            return dst;
        }
    }

    endian_num<T, std::endian::little> to_le() {
        if constexpr (ENDIAN == std::endian::little) {
            return *this;
        } else {
            return get();
        }
    }

    endian_num<T, std::endian::big> to_be() {
        if constexpr (ENDIAN == std::endian::big) {
            return *this;
        } else {
            return get();
        }
    }

    endian_num() : value(0) {}

    // friend std::ostream& operator<<(std::ostream& os, const T& obj) {
    //     return os << obj.get();
    // }

    // friend std::istream& operator>> (std::istream& in, T& other)
    // {
    //   in >> other.get();
    //   return in;
    // }

#undef BQ_DEF
#define BQ_DEF(kind, getter)                                                                       \
    endian_num(const kind& v) : value(set(static_cast<T>(getter))) {}                              \
    T operator=(const kind v) {                                                                    \
        return set(static_cast<T>(getter));                                                        \
    }                                                                                              \
    T operator+=(const kind v) {                                                                   \
        return set(get() + static_cast<T>(getter));                                                \
    }                                                                                              \
    T operator-=(const kind v) {                                                                   \
        return set(get() - static_cast<T>(getter));                                                \
    }                                                                                              \
    T operator*=(const kind v) {                                                                   \
        return set(get() * static_cast<T>(getter));                                                \
    }                                                                                              \
    T operator/=(const kind v) {                                                                   \
        return set(get() / static_cast<T>(getter));                                                \
    }                                                                                              \
    T operator%=(const kind v) {                                                                   \
        return set(get() % static_cast<T>(getter));                                                \
    }                                                                                              \
    T operator&=(const kind v) {                                                                   \
        return set(get() & static_cast<T>(getter));                                                \
    }                                                                                              \
    T operator|=(const kind v) {                                                                   \
        return set(get() | static_cast<T>(getter));                                                \
    }                                                                                              \
    T operator^=(const kind v) {                                                                   \
        return set(get() ^ static_cast<T>(getter));                                                \
    }                                                                                              \
    T operator+(const kind v) const {                                                              \
        return get() + static_cast<T>(getter);                                                     \
    }                                                                                              \
    T operator-(const kind v) const {                                                              \
        return get() - static_cast<T>(getter);                                                     \
    }                                                                                              \
    T operator*(const kind v) const {                                                              \
        return get() * static_cast<T>(getter);                                                     \
    }                                                                                              \
    T operator/(const kind v) const {                                                              \
        return get() / static_cast<T>(getter);                                                     \
    }                                                                                              \
    T operator%(const kind v) const {                                                              \
        return get() % static_cast<T>(getter);                                                     \
    }                                                                                              \
    T operator&(const kind v) const {                                                              \
        return get() & static_cast<T>(getter);                                                     \
    }                                                                                              \
    T operator|(const kind v) const {                                                              \
        return get() & static_cast<T>(getter);                                                     \
    }                                                                                              \
    T operator^(const kind v) const {                                                              \
        return get() ^ static_cast<T>(getter);                                                     \
    }                                                                                              \
    bool operator==(const kind v) const {                                                          \
        return static_cast<kind>(get()) == (getter);                                               \
    }                                                                                              \
    bool operator!=(const kind v) const {                                                          \
        return static_cast<kind>(get()) != (getter);                                               \
    }                                                                                              \
    bool operator<(const kind v) const {                                                           \
        return static_cast<kind>(get()) < (getter);                                                \
    }                                                                                              \
    bool operator>(const kind v) const {                                                           \
        return static_cast<kind>(get()) > (getter);                                                \
    }                                                                                              \
    bool operator<=(const kind v) const {                                                          \
        return static_cast<kind>(get()) <= (getter);                                               \
    }                                                                                              \
    bool operator>=(const kind v) const {                                                          \
        return static_cast<kind>(get()) >= (getter);                                               \
    }                                                                                              \
    std::partial_ordering operator<=>(const kind v) const {                                        \
        return static_cast<kind>(get()) <=> (getter);                                              \
    }

#define COMMA ,
    BQ_DEF(std::int8_t, v)
    BQ_DEF(std::int16_t, v)
    BQ_DEF(std::int32_t, v)
    BQ_DEF(std::int64_t, v)
    BQ_DEF(std::uint8_t, v)
    BQ_DEF(std::uint16_t, v)
    BQ_DEF(std::uint32_t, v)
    BQ_DEF(std::uint64_t, v)
    BQ_DEF(std::float_t, v)
    BQ_DEF(std::double_t, v)
    BQ_DEF(endian_num<std::int16_t COMMA std::endian::little>, v.get());
    BQ_DEF(endian_num<std::int32_t COMMA std::endian::little>, v.get());
    BQ_DEF(endian_num<std::int64_t COMMA std::endian::little>, v.get());
    BQ_DEF(endian_num<std::uint16_t COMMA std::endian::little>, v.get());
    BQ_DEF(endian_num<std::uint32_t COMMA std::endian::little>, v.get());
    BQ_DEF(endian_num<std::uint64_t COMMA std::endian::little>, v.get());
    BQ_DEF(endian_num<std::float_t COMMA std::endian::little>, v.get());
    BQ_DEF(endian_num<std::double_t COMMA std::endian::little>, v.get());
    BQ_DEF(endian_num<std::int16_t COMMA std::endian::big>, v.get());
    BQ_DEF(endian_num<std::int32_t COMMA std::endian::big>, v.get());
    BQ_DEF(endian_num<std::int64_t COMMA std::endian::big>, v.get());
    BQ_DEF(endian_num<std::uint16_t COMMA std::endian::big>, v.get());
    BQ_DEF(endian_num<std::uint32_t COMMA std::endian::big>, v.get());
    BQ_DEF(endian_num<std::uint64_t COMMA std::endian::big>, v.get());
    BQ_DEF(endian_num<std::float_t COMMA std::endian::big>, v.get());
    BQ_DEF(endian_num<std::double_t COMMA std::endian::big>, v.get());
#undef COMMA

#undef BQ_DEF

  private:
    T value;
};

using i8 = std::int8_t;
using i16 = std::int16_t;
using i32 = std::int32_t;
using i64 = std::int64_t;
using u8 = std::uint8_t;
using u16 = std::uint16_t;
using u32 = std::uint32_t;
using u64 = std::uint64_t;
using f32 = std::float_t;
using f64 = std::double_t;
using usize = std::size_t;
using i16ne = endian_num<std::int16_t, std::endian::native>;
using i32ne = endian_num<std::int32_t, std::endian::native>;
using i64ne = endian_num<std::int64_t, std::endian::native>;
using u16ne = endian_num<std::uint16_t, std::endian::native>;
using u32ne = endian_num<std::uint32_t, std::endian::native>;
using u64ne = endian_num<std::uint64_t, std::endian::native>;
using f32ne = endian_num<std::float_t, std::endian::native>;
using f64ne = endian_num<std::double_t, std::endian::native>;
using i16le = endian_num<std::int16_t, std::endian::little>;
using i32le = endian_num<std::int32_t, std::endian::little>;
using i64le = endian_num<std::int64_t, std::endian::little>;
using u16le = endian_num<std::uint16_t, std::endian::little>;
using u32le = endian_num<std::uint32_t, std::endian::little>;
using u64le = endian_num<std::uint64_t, std::endian::little>;
using f32le = endian_num<std::float_t, std::endian::little>;
using f64le = endian_num<std::double_t, std::endian::little>;
using i16be = endian_num<std::int16_t, std::endian::big>;
using i32be = endian_num<std::int32_t, std::endian::big>;
using i64be = endian_num<std::int64_t, std::endian::big>;
using u16be = endian_num<std::uint16_t, std::endian::big>;
using u32be = endian_num<std::uint32_t, std::endian::big>;
using u64be = endian_num<std::uint64_t, std::endian::big>;
using f32be = endian_num<std::float_t, std::endian::big>;
using f64be = endian_num<std::double_t, std::endian::big>;

inline i16le from_le(i16 v) {
    return v;
}
inline i32le from_le(i32 v) {
    return v;
}
inline i64le from_le(i64 v) {
    return v;
}
inline u16le from_le(u16 v) {
    return v;
}
inline u32le from_le(u32 v) {
    return v;
}
inline u64le from_le(u64 v) {
    return v;
}
inline f32le from_le(f32 v) {
    return v;
}
inline f64le from_le(f64 v) {
    return v;
}

inline i16be from_be(i16 v) {
    return v;
}
inline i32be from_be(i32 v) {
    return v;
}
inline i64be from_be(i64 v) {
    return v;
}
inline u16be from_be(u16 v) {
    return v;
}
inline u32be from_be(u32 v) {
    return v;
}
inline u64be from_be(u64 v) {
    return v;
}
inline f32be from_be(f32 v) {
    return v;
}
inline f64be from_be(f64 v) {
    return v;
}

template <typename T>
endian_num<T, std::endian::native> as;

using milliseconds = std::chrono::milliseconds;

// using str    = std::basic_string<char, std::char_traits<char>, mi_stl_allocator<char>>;
using str = std::string;
using string = str;
using bytes = str;

template <typename T>
using vec = std::vector<T>;

template <typename T>
using queue = std::queue<T>;

using error_code = std::error_code;

template <class T, class E>
using expected = std::expected<T, E>;

template <class E>
using unexpected = std::unexpected<E>;
} // namespace bisque
