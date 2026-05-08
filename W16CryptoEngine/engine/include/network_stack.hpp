#pragma once

#include "hft_core.hpp"
#include "spsc_queue.hpp"
#include <cstdint>
#include <cstring>
#include <unistd.h>
#include <fcntl.h>
#include <sys/socket.h>
#include <netinet/tcp.h>
#include <arpa/inet.h>
#include <netdb.h>

#ifdef USE_IO_URING
#include <liburing.h>
#endif

namespace hft {

static constexpr size_t RECV_BUFFER_SIZE = 65536;
static constexpr size_t MAX_CONNECTIONS = 8;
static constexpr int TCP_NODELAY_FLAG = 1;
static constexpr int TCP_QUICKACK = 1;

struct alignas(64) NetworkBuffer {
    char data[RECV_BUFFER_SIZE];
    size_t len;
    size_t consumed;
    uint64_t timestamp_ns;

    NetworkBuffer() : len(0), consumed(0), timestamp_ns(0) {}
};

class NetworkStack {
    int socket_fd_;
    alignas(64) NetworkBuffer recv_buffer_;

#ifdef USE_IO_URING
    struct io_uring ring_;
    bool use_io_uring_;
#endif

public:
    NetworkStack() : socket_fd_(-1) {
#ifdef USE_IO_URING
        use_io_uring_ = false;
#endif
    }

    bool connect(const char* host, uint16_t port) noexcept {
        socket_fd_ = socket(AF_INET, SOCK_STREAM, 0);
        if (socket_fd_ < 0) {
            return false;
        }

        int flags = fcntl(socket_fd_, F_GETFL, 0);
        if (flags < 0) {
            close(socket_fd_);
            socket_fd_ = -1;
            return false;
        }

        if (fcntl(socket_fd_, F_SETFL, flags | O_NONBLOCK) < 0) {
            close(socket_fd_);
            socket_fd_ = -1;
            return false;
        }

        int opt = TCP_NODELAY_FLAG;
        setsockopt(socket_fd_, IPPROTO_TCP, TCP_NODELAY, &opt, sizeof(opt));

#ifdef TCP_QUICKACK
        opt = TCP_QUICKACK;
        setsockopt(socket_fd_, IPPROTO_TCP, TCP_QUICKACK, &opt, sizeof(opt));
#endif

        struct sockaddr_in addr;
        std::memset(&addr, 0, sizeof(addr));
        addr.sin_family = AF_INET;
        addr.sin_port = htons(port);

        if (inet_pton(AF_INET, host, &addr.sin_addr) <= 0) {
            struct hostent* he = gethostbyname(host);
            if (!he || !he->h_addr_list[0]) {
                close(socket_fd_);
                socket_fd_ = -1;
                return false;
            }
            std::memcpy(&addr.sin_addr, he->h_addr_list[0], sizeof(struct in_addr));
        }

        if (::connect(socket_fd_, reinterpret_cast<struct sockaddr*>(&addr), sizeof(addr)) < 0) {
            if (errno != EINPROGRESS) {
                close(socket_fd_);
                socket_fd_ = -1;
                return false;
            }
        }

        fd_set fdset;
        FD_ZERO(&fdset);
        FD_SET(socket_fd_, &fdset);

        struct timeval tv;
        tv.tv_sec = 5;
        tv.tv_usec = 0;

        if (select(socket_fd_ + 1, nullptr, &fdset, nullptr, &tv) <= 0) {
            close(socket_fd_);
            socket_fd_ = -1;
            return false;
        }

        int so_error;
        socklen_t len = sizeof(so_error);
        getsockopt(socket_fd_, SOL_SOCKET, SO_ERROR, &so_error, &len);

        if (so_error != 0) {
            close(socket_fd_);
            socket_fd_ = -1;
            return false;
        }

        flags = fcntl(socket_fd_, F_GETFL, 0);
        fcntl(socket_fd_, F_SETFL, flags & ~O_NONBLOCK);

#ifdef USE_IO_URING
        if (io_uring_queue_init(128, &ring_, 0) == 0) {
            use_io_uring_ = true;
        }
#endif

        return true;
    }

    bool send(const char* data, size_t len) noexcept {
        if (socket_fd_ < 0) {
            return false;
        }

        size_t total_sent = 0;
        while (total_sent < len) {
            ssize_t sent = ::send(socket_fd_, data + total_sent, len - total_sent, MSG_NOSIGNAL);
            if (sent < 0) {
                if (errno == EAGAIN || errno == EWOULDBLOCK) {
                    continue;
                }
                return false;
            }
            total_sent += static_cast<size_t>(sent);
        }
        return true;
    }

    bool receive(NetworkBuffer& buffer) noexcept {
        if (socket_fd_ < 0) {
            return false;
        }

        buffer.timestamp_ns = rdtsc();

#ifdef USE_IO_URING
        if (use_io_uring_) {
            return receive_io_uring(buffer);
        }
#endif

        buffer.len = 0;
        buffer.consumed = 0;

        ssize_t received = recv(socket_fd_, buffer.data, RECV_BUFFER_SIZE, 0);
        if (received <= 0) {
            if (received < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) {
                return true;
            }
            return false;
        }

        buffer.len = static_cast<size_t>(received);
        return true;
    }

    void disconnect() noexcept {
        if (socket_fd_ >= 0) {
            close(socket_fd_);
            socket_fd_ = -1;
        }

#ifdef USE_IO_URING
        if (use_io_uring_) {
            io_uring_queue_exit(&ring_);
            use_io_uring_ = false;
        }
#endif
    }

private:
#ifdef USE_IO_URING
    bool receive_io_uring(NetworkBuffer& buffer) noexcept {
        struct io_uring_sqe* sqe = io_uring_get_sqe(&ring_);
        if (!sqe) {
            return receive_epoll(buffer);
        }

        io_uring_prep_recv(sqe, socket_fd_, buffer.data, RECV_BUFFER_SIZE, 0);
        io_uring_sqe_set_data(sqe, &buffer);
        io_uring_submit(&ring_);

        struct io_uring_cqe* cqe;
        int ret = io_uring_wait_cqe(&ring_, &cqe);
        if (ret < 0) {
            return false;
        }

        if (cqe->res < 0) {
            io_uring_cqe_seen(&ring_, cqe);
            return false;
        }

        buffer.len = static_cast<size_t>(cqe->res);
        io_uring_cqe_seen(&ring_, cqe);
        return true;
    }
#endif

    bool receive_epoll(NetworkBuffer& buffer) noexcept {
        buffer.len = 0;
        buffer.consumed = 0;

        ssize_t received = recv(socket_fd_, buffer.data, RECV_BUFFER_SIZE, 0);
        if (received <= 0) {
            if (received < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) {
                return true;
            }
            return false;
        }

        buffer.len = static_cast<size_t>(received);
        return true;
    }
};

}
