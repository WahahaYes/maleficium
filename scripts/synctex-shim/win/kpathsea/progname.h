/* Stand-in for kpathsea's progname.h: MinGW defines WIN32, which makes
 * synctex_main.c register its name with kpathsea. The standalone CLI has
 * no kpathsea to register with. */
#ifndef MALEFICIUM_SYNCTEX_PROGNAME_H
#define MALEFICIUM_SYNCTEX_PROGNAME_H
#define kpse_set_program_name(argv0, progname) ((void)(argv0), (void)(progname))
#endif
