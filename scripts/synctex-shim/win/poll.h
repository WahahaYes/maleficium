/* Stand-in for <poll.h>, which MinGW lacks. synctex_main.c uses poll()
 * only in its interactive mode, which the app never starts; reporting no
 * pending input keeps that mode on its blocking read(). */
#ifndef MALEFICIUM_SYNCTEX_POLL_H
#define MALEFICIUM_SYNCTEX_POLL_H
#define POLLIN 0x0001
struct pollfd {
    int fd;
    short events;
    short revents;
};
static inline int poll(struct pollfd *fds, unsigned long nfds, int timeout)
{
    (void)fds;
    (void)nfds;
    (void)timeout;
    return 0;
}
#endif
