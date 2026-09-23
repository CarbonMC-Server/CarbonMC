/* Stable runtime defaults for the statically vendored OpenSSL release build.
 * Configure's install prefix is a temporary Cargo directory. It must not
 * become a runtime search path or make independent release binaries differ.
 * Installation still uses Configure's real prefix; only compiled defaults
 * change. Provider modules are disabled by openssl-src's no-module build.
 */
/* Keep absolute MSVC source paths out of OpenSSL error records. */
#define OPENSSL_NO_FILENAMES
#undef OPENSSLDIR
#undef ENGINESDIR
#undef MODULESDIR
#ifdef _WIN32
#define OPENSSLDIR "C:/Program Files/Common Files/SSL"
#define ENGINESDIR "C:/Program Files/Common Files/SSL/engines-3"
#define MODULESDIR "C:/Program Files/Common Files/SSL/ossl-modules"
#else
#define OPENSSLDIR "/usr/local/ssl"
#define ENGINESDIR "/usr/local/lib/engines-3"
#define MODULESDIR "/usr/local/lib/ossl-modules"
#endif
