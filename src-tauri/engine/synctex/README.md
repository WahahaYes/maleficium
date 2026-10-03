# Vendored SyncTeX parser

The SyncTeX parser (`synctex_parser.c`, `synctex_parser_utils.c` and their headers) from <https://github.com/jlaurens/synctex> at commit `04cf8e3e8665ff203248d7af78ee1129afbc1b64`, MIT licensed (`LICENSE`). Files are unmodified. `w2c/c-auto.h` is our stand-in for web2c's generated header: the parser includes it and needs nothing from it on the platforms we build.

`../build.rs` compiles the two C files into `maleficium-engine`, and `../src/synctex.rs` is the `synctex` subcommand over the parser's query API. The upstream command line tool (`synctex_main.c`) is not used: it needs POSIX headers MSVC lacks and can run arbitrary commands through `-x`.

To update: copy the same files from a newer upstream commit and change the commit above.
