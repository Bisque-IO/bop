/* This is a simple yet efficient HTTP server benchmark */
#include <libusockets.h>
/* If compiled with SSL support, enable it */
const int SSL = 1;

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

char request_template[] = "GET / HTTP/1.1\r\nHost: localhost:3000\r\nUser-Agent: curl/7.68.0\r\nAccept: */*\r\n\r\n";
char request_template_post[] = "POST / HTTP/1.1\r\nHost: localhost:3000\r\nUser-Agent: curl/7.68.0\r\nAccept: */*\r\nContent-Length: 10\r\n\r\n{\"key\":13}";

char *request;
int request_size;
char *host;
int port;
int connections;

int responses;
int pipeline = 1;
int is_post = 0;

struct http_socket {
    /* How far we have streamed our request */
    int offset;
};

/* We don't need any of these */
void on_wakeup(struct us_loop_t *loop) {

}

void on_pre(struct us_loop_t *loop) {

}

/* This is not HTTP POST, it is merely an event emitted post loop iteration */
void on_post(struct us_loop_t *loop) {

}

struct us_socket_t *on_http_socket_writable(struct us_socket_t *s) {
    struct http_socket *http_socket = (struct http_socket *) us_socket_ext(SSL, s);

    /* Stream whatever is remaining of the request */
    http_socket->offset += us_socket_write(SSL, s, request + http_socket->offset, (request_size) - http_socket->offset, 0);

    return s;
}

struct us_socket_t *on_http_socket_close(struct us_socket_t *s, int code, void *reason) {
	return s;
}

struct us_socket_t *on_http_socket_end(struct us_socket_t *s) {
    return us_socket_close(SSL, s, 0, NULL);
}

struct us_socket_t *on_http_socket_data(struct us_socket_t *s, char *data, int length) {
    /* Get socket extension and the socket's context's extension */
    struct http_socket *http_socket = (struct http_socket *) us_socket_ext(SSL, s);

    /* We treat all data events as a response */
    http_socket->offset = us_socket_write(SSL, s, request, request_size, 0);

    /* */
    responses++;

    return s;
}

struct us_socket_t *on_http_socket_open(struct us_socket_t *s, int is_client, char *ip, int ip_length) {
    struct http_socket *http_socket = (struct http_socket *) us_socket_ext(SSL, s);

    /* Reset offset */
    http_socket->offset = 0;

    /* Send a request */
    us_socket_write(SSL, s, request, request_size, 0);

    if (--connections) {
        us_socket_context_connect(SSL, us_socket_context(SSL, s), host, port, NULL, 0, sizeof(struct http_socket));
    } else {
        printf("Running benchmark now...\n");

        us_socket_timeout(SSL, s, LIBUS_TIMEOUT_GRANULARITY);
        us_socket_long_timeout(SSL, s, 1);
    }

    return s;
}

struct us_socket_t *on_http_socket_long_timeout(struct us_socket_t *s) {
    /* Print current statistics */
    printf("--- Minute mark ---\n");
    us_socket_long_timeout(SSL, s, 1);

    return s;
}

struct us_socket_t *on_http_socket_timeout(struct us_socket_t *s) {
    /* Print current statistics */
    printf("Req/sec: %f\n", ((float)pipeline) * ((float)responses) / LIBUS_TIMEOUT_GRANULARITY);

    responses = 0;
    us_socket_timeout(SSL, s, LIBUS_TIMEOUT_GRANULARITY);

    return s;
}

struct us_socket_t *on_http_socket_connect_error(struct us_socket_t *s, int code) {
    printf("Cannot connect to server\n");

    return s;
}

int main(int argc, char **argv) {

    /* Parse host and port */
    if (argc != 5 && argc != 4 && argc != 6) {
        printf("Usage: connections host port [pipeline factor] [with body]\n");
        return 0;
    }

    if (argc >= 5) {
        pipeline =  atoi(argv[4]);
        printf("Using pipeline factor of %d\n", pipeline);
    }

    const char *selected_request = request_template;
    int selected_request_size = sizeof(request_template) - 1;

    if (argc >= 6) {
        is_post =  atoi(argv[5]);
        printf("Using post with body\n");

        selected_request = request_template_post;
        selected_request_size = sizeof(request_template_post) - 1;
    }
    /* Pipeline to 16 */
    request_size = pipeline * selected_request_size;
    printf("request size %d\n", request_size);
    request = malloc(request_size);
    for (int i = 0; i < pipeline; i++) {
        memcpy(request + i * selected_request_size, selected_request, selected_request_size);
    }

    port = atoi(argv[3]);
    host = malloc(strlen(argv[2]) + 1);
    memcpy(host, argv[2], strlen(argv[2]) + 1);
    connections = atoi(argv[1]);

    /* Create the event loop */
    struct us_loop_t *loop = us_create_loop(0, on_wakeup, on_pre, on_post, 0);

    /* Create a socket context for HTTP */
    struct us_socket_context_options_t options = {};
    options.key_file_name = "key.pem";
    options.cert_file_name = "cert.pem";
    options.passphrase = "1234";
    struct us_socket_context_t *http_context = us_create_socket_context(SSL, loop, 0, options);

    if (!http_context) {
		printf("Could not load SSL cert/key\n");
		exit(0);
	}

    /* Set up event handlers */
    us_socket_context_on_open(SSL, http_context, on_http_socket_open);
    us_socket_context_on_data(SSL, http_context, on_http_socket_data);
    us_socket_context_on_writable(SSL, http_context, on_http_socket_writable);
    us_socket_context_on_close(SSL, http_context, on_http_socket_close);
    us_socket_context_on_timeout(SSL, http_context, on_http_socket_timeout);
    us_socket_context_on_long_timeout(SSL, http_context, on_http_socket_long_timeout);
    us_socket_context_on_end(SSL, http_context, on_http_socket_end);
    us_socket_context_on_connect_error(SSL, http_context, on_http_socket_connect_error);

    /* Start making HTTP connections */
    if (!us_socket_context_connect(SSL, http_context, host, port, NULL, 0, sizeof(struct http_socket))) {
        printf("Cannot connect to server\n");
    }

    us_loop_run(loop);

    us_socket_context_free(SSL, http_context);
    us_loop_free(loop);
}
