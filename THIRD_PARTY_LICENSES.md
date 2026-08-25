# Third-party license inventory

Generated deterministically from target-filtered `cargo metadata --locked` and `pnpm licenses list --prod` by `pnpm licenses:generate`. Workspace-owned BA Agent packages and development-only Cargo dependencies are excluded.

Rust target union: `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`.

This inventory records package license expressions and upstream references. Before Public Beta, release review must verify every expression and include any license or notice text required by the corresponding terms.

## Rust runtime packages (273)

| Package | Version | License | Upstream |
| --- | --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | [source](https://github.com/oyvindln/adler2) |
| aes | 0.8.4 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/block-ciphers) |
| aho-corasick | 1.1.5 | Unlicense OR MIT | [source](https://github.com/BurntSushi/aho-corasick) |
| anstream | 1.0.0 | MIT OR Apache-2.0 | [source](https://github.com/rust-cli/anstyle.git) |
| anstyle | 1.0.14 | MIT OR Apache-2.0 | [source](https://github.com/rust-cli/anstyle.git) |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 | [source](https://github.com/rust-cli/anstyle.git) |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 | [source](https://github.com/rust-cli/anstyle.git) |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 | [source](https://github.com/rust-cli/anstyle.git) |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/anyhow) |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 | [source](https://github.com/smol-rs/async-broadcast) |
| async-compression | 0.4.43 | MIT OR Apache-2.0 | [source](https://github.com/Nullus157/async-compression) |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/async-trait) |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | [source](https://github.com/smol-rs/atomic-waker) |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | [source](https://github.com/cuviper/autocfg) |
| axum | 0.8.9 | MIT | [source](https://github.com/tokio-rs/axum) |
| axum-core | 0.5.6 | MIT | [source](https://github.com/tokio-rs/axum) |
| axum-macros | 0.5.1 | MIT | [source](https://github.com/tokio-rs/axum) |
| base64 | 0.22.1 | MIT OR Apache-2.0 | [source](https://github.com/marshallpierce/rust-base64) |
| bitflags | 2.13.1 | MIT OR Apache-2.0 | [source](https://github.com/bitflags/bitflags) |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/utils) |
| block-padding | 0.3.3 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/utils) |
| byteorder | 1.5.0 | Unlicense OR MIT | [source](https://github.com/BurntSushi/byteorder) |
| bytes | 1.12.1 | MIT | [source](https://github.com/tokio-rs/bytes) |
| cbc | 0.1.2 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/block-modes) |
| cc | 1.4.4 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/cc-rs) |
| cfg-if | 1.0.4 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/cfg-if) |
| cfg_aliases | 0.2.2 | MIT | [source](https://github.com/katharostech/cfg_aliases) |
| chacha20 | 0.10.1 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/stream-ciphers) |
| chrono | 0.4.45 | MIT OR Apache-2.0 | [source](https://github.com/chronotope/chrono) |
| cipher | 0.4.4 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/traits) |
| clap | 4.6.6 | MIT OR Apache-2.0 | [source](https://github.com/clap-rs/clap) |
| clap_builder | 4.6.6 | MIT OR Apache-2.0 | [source](https://github.com/clap-rs/clap) |
| clap_derive | 4.6.4 | MIT OR Apache-2.0 | [source](https://github.com/clap-rs/clap) |
| clap_lex | 1.1.0 | MIT OR Apache-2.0 | [source](https://github.com/clap-rs/clap) |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 | [source](https://github.com/rust-cli/anstyle.git) |
| compression-codecs | 0.4.38 | MIT OR Apache-2.0 | [source](https://github.com/Nullus157/async-compression) |
| compression-core | 0.4.32 | MIT OR Apache-2.0 | [source](https://github.com/Nullus157/async-compression) |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | [source](https://github.com/servo/core-foundation-rs) |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | [source](https://github.com/servo/core-foundation-rs) |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/utils) |
| cpufeatures | 0.3.0 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/utils) |
| crc32fast | 1.5.1 | MIT OR Apache-2.0 | [source](https://github.com/srijs/rust-crc32fast) |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/traits) |
| dbus | 0.9.12 | Apache-2.0/MIT | [source](https://github.com/diwic/dbus-rs) |
| dbus-secret-service | 4.1.0 | MIT OR Apache-2.0 | [source](https://github.com/brotskydotcom/dbus-secret-service.git) |
| digest | 0.10.7 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/traits) |
| directories | 6.0.0 | MIT OR Apache-2.0 | [source](https://github.com/soc/directories-rs) |
| dirs-sys | 0.5.0 | MIT OR Apache-2.0 | [source](https://github.com/dirs-dev/dirs-sys-rs) |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | [source](https://github.com/yaahc/displaydoc) |
| endi | 1.1.1 | MIT | [source](https://github.com/zeenix/endi) |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 | [source](https://github.com/meithecatte/enumflags2) |
| enumflags2_derive | 0.7.12 | MIT OR Apache-2.0 | [source](https://github.com/meithecatte/enumflags2) |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | [source](https://github.com/indexmap-rs/equivalent) |
| errno | 0.3.14 | MIT OR Apache-2.0 | [source](https://github.com/lambda-fairy/rust-errno) |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | [source](https://github.com/smol-rs/event-listener) |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | [source](https://github.com/smol-rs/event-listener-strategy) |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 | [source](https://github.com/sfackler/rust-fallible-iterator) |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 | [source](https://github.com/sfackler/fallible-streaming-iterator) |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | [source](https://github.com/smol-rs/fastrand) |
| find-msvc-tools | 0.1.11 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/cc-rs) |
| flate2 | 1.1.9 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/flate2-rs) |
| fnv | 1.0.7 | Apache-2.0 / MIT | [source](https://github.com/servo/rust-fnv) |
| foldhash | 0.1.5 | Zlib | [source](https://github.com/orlp/foldhash) |
| foreign-types | 0.3.2 | MIT/Apache-2.0 | [source](https://github.com/sfackler/foreign-types) |
| foreign-types-shared | 0.1.1 | MIT/Apache-2.0 | [source](https://github.com/sfackler/foreign-types) |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | [source](https://github.com/servo/rust-url) |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/futures-rs) |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/futures-rs) |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/futures-rs) |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/futures-rs) |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/futures-rs) |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/futures-rs) |
| generic-array | 0.14.7 | MIT | [source](https://github.com/fizyk20/generic-array.git) |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/getrandom) |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/getrandom) |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/getrandom) |
| h2 | 0.4.18 | MIT | [source](https://github.com/hyperium/h2) |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/hashbrown) |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/hashbrown) |
| hashlink | 0.10.0 | MIT OR Apache-2.0 | [source](https://github.com/kyren/hashlink) |
| heck | 0.5.0 | MIT OR Apache-2.0 | [source](https://github.com/withoutboats/heck) |
| hex | 0.4.3 | MIT OR Apache-2.0 | [source](https://github.com/KokaKiwi/rust-hex) |
| hkdf | 0.12.4 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/KDFs/) |
| hmac | 0.12.1 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/MACs) |
| http | 1.5.0 | MIT OR Apache-2.0 | [source](https://github.com/hyperium/http) |
| http-body | 1.1.0 | MIT | [source](https://github.com/hyperium/http-body) |
| http-body-util | 0.1.5 | MIT | [source](https://github.com/hyperium/http-body) |
| http-range-header | 0.4.2 | MIT | [source](https://github.com/MarcusGrass/parse-range-headers) |
| httparse | 1.10.1 | MIT OR Apache-2.0 | [source](https://github.com/seanmonstar/httparse) |
| httpdate | 1.0.3 | MIT OR Apache-2.0 | [source](https://github.com/pyfisch/httpdate) |
| hyper | 1.11.0 | MIT | [source](https://github.com/hyperium/hyper) |
| hyper-rustls | 0.27.9 | Apache-2.0 OR ISC OR MIT | [source](https://github.com/rustls/hyper-rustls) |
| hyper-util | 0.1.20 | MIT | [source](https://github.com/hyperium/hyper-util) |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | [source](https://github.com/strawlab/iana-time-zone) |
| icu_collections | 2.1.1 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| icu_locale_core | 2.1.1 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| icu_normalizer | 2.1.1 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| icu_normalizer_data | 2.1.1 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| icu_properties | 2.1.2 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| icu_properties_data | 2.1.2 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| icu_provider | 2.1.1 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| idna | 1.1.0 | MIT OR Apache-2.0 | [source](https://github.com/servo/rust-url/) |
| idna_adapter | 1.2.1 | Apache-2.0 OR MIT | [source](https://github.com/hsivonen/idna_adapter) |
| indexmap | 2.14.0 | Apache-2.0 OR MIT | [source](https://github.com/indexmap-rs/indexmap) |
| inout | 0.1.4 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/utils) |
| ipnet | 2.12.1 | MIT OR Apache-2.0 | [source](https://github.com/krisprice/ipnet) |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 | [source](https://github.com/polyfill-rs/is_terminal_polyfill) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/itoa) |
| keyring | 3.6.3 | MIT OR Apache-2.0 | [source](https://github.com/hwchen/keyring-rs.git) |
| lazy_static | 1.5.0 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang-nursery/lazy-static.rs) |
| libc | 0.2.189 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/libc) |
| libdbus-sys | 0.2.7 | Apache-2.0/MIT | [source](https://github.com/diwic/dbus-rs) |
| libsqlite3-sys | 0.35.0 | MIT | [source](https://github.com/rusqlite/rusqlite) |
| litemap | 0.8.3 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | [source](https://github.com/Amanieu/parking_lot) |
| log | 0.4.34 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/log) |
| lru-slab | 0.1.2 | MIT OR Apache-2.0 OR Zlib | [source](https://github.com/Ralith/lru-slab) |
| matchers | 0.2.0 | MIT | [source](https://github.com/hawkw/matchers) |
| matchit | 0.8.4 | MIT AND BSD-3-Clause | [source](https://github.com/ibraheemdev/matchit) |
| memchr | 2.8.3 | Unlicense OR MIT | [source](https://github.com/BurntSushi/memchr) |
| memoffset | 0.9.1 | MIT | [source](https://github.com/Gilnaa/memoffset) |
| mime | 0.3.17 | MIT OR Apache-2.0 | [source](https://github.com/hyperium/mime) |
| mime_guess | 2.0.5 | MIT | [source](https://github.com/abonander/mime_guess) |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | [source](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) |
| mio | 1.2.2 | MIT | [source](https://github.com/tokio-rs/mio) |
| nix | 0.29.0 | MIT | [source](https://github.com/nix-rust/nix) |
| nu-ansi-term | 0.50.3 | MIT | [source](https://github.com/nushell/nu-ansi-term) |
| num | 0.4.3 | MIT OR Apache-2.0 | [source](https://github.com/rust-num/num) |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 | [source](https://github.com/rust-num/num-bigint) |
| num-complex | 0.4.6 | MIT OR Apache-2.0 | [source](https://github.com/rust-num/num-complex) |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | [source](https://github.com/rust-num/num-integer) |
| num-iter | 0.1.46 | MIT OR Apache-2.0 | [source](https://github.com/rust-num/num-iter) |
| num-rational | 0.4.2 | MIT OR Apache-2.0 | [source](https://github.com/rust-num/num-rational) |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | [source](https://github.com/rust-num/num-traits) |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | [source](https://github.com/matklad/once_cell) |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 | [source](https://github.com/polyfill-rs/once_cell_polyfill) |
| openssl | 0.10.81 | Apache-2.0 | [source](https://github.com/rust-openssl/rust-openssl) |
| openssl-macros | 0.1.1 | MIT/Apache-2.0 | `registry+https://github.com/rust-lang/crates.io-index` |
| openssl-src | 300.6.1+3.6.3 | MIT/Apache-2.0 | [source](https://github.com/alexcrichton/openssl-src-rs) |
| openssl-sys | 0.9.117 | MIT | [source](https://github.com/rust-openssl/rust-openssl) |
| option-ext | 0.2.0 | MPL-2.0 | [source](https://github.com/soc/option-ext.git) |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 | [source](https://github.com/danieldg/ordered-stream) |
| parking | 2.2.1 | Apache-2.0 OR MIT | [source](https://github.com/smol-rs/parking) |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | [source](https://github.com/Amanieu/parking_lot) |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | [source](https://github.com/Amanieu/parking_lot) |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | [source](https://github.com/servo/rust-url/) |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | [source](https://github.com/taiki-e/pin-project-lite) |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/pkg-config-rs) |
| potential_utf | 0.1.6 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | [source](https://github.com/cryptocorrosion/cryptocorrosion) |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | [source](https://github.com/bkchr/proc-macro-crate) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/proc-macro2) |
| quinn | 0.11.11 | MIT OR Apache-2.0 | [source](https://github.com/quinn-rs/quinn) |
| quinn-proto | 0.11.17 | MIT OR Apache-2.0 | [source](https://github.com/quinn-rs/quinn) |
| quinn-udp | 0.5.15 | MIT OR Apache-2.0 | [source](https://github.com/quinn-rs/quinn) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/quote) |
| rand | 0.10.2 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/rand) |
| rand | 0.8.7 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/rand) |
| rand | 0.9.5 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/rand) |
| rand_chacha | 0.3.1 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/rand) |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/rand) |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/rand_core) |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/rand) |
| rand_core | 0.9.5 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/rand) |
| rand_pcg | 0.10.2 | MIT OR Apache-2.0 | [source](https://github.com/rust-random/rngs) |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/regex) |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/regex) |
| reqwest | 0.12.28 | MIT OR Apache-2.0 | [source](https://github.com/seanmonstar/reqwest) |
| ring | 0.17.14 | Apache-2.0 AND ISC | [source](https://github.com/briansmith/ring) |
| rusqlite | 0.37.0 | MIT | [source](https://github.com/rusqlite/rusqlite) |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | [source](https://github.com/rust-lang/rustc-hash) |
| rustls | 0.23.43 | Apache-2.0 OR ISC OR MIT | [source](https://github.com/rustls/rustls) |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | [source](https://github.com/rustls/pki-types) |
| rustls-webpki | 0.103.15 | ISC | [source](https://github.com/rustls/webpki) |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | [source](https://github.com/dtolnay/ryu) |
| same-file | 1.0.6 | Unlicense/MIT | [source](https://github.com/BurntSushi/same-file) |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | [source](https://github.com/bluss/scopeguard) |
| secret-service | 4.0.0 | MIT OR Apache-2.0 | [source](https://github.com/hwchen/secret-service-rs.git) |
| security-framework | 3.7.0 | MIT OR Apache-2.0 | [source](https://github.com/kornelski/rust-security-framework) |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 | [source](https://github.com/kornelski/rust-security-framework) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [source](https://github.com/serde-rs/serde) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [source](https://github.com/serde-rs/serde) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [source](https://github.com/serde-rs/serde) |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | [source](https://github.com/serde-rs/json) |
| serde_path_to_error | 0.1.20 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/path-to-error) |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/serde-repr) |
| serde_spanned | 1.1.1 | MIT OR Apache-2.0 | [source](https://github.com/toml-rs/toml) |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | [source](https://github.com/nox/serde_urlencoded) |
| sha1 | 0.10.7 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/hashes) |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | [source](https://github.com/RustCrypto/hashes) |
| sharded-slab | 0.1.7 | MIT | [source](https://github.com/hawkw/sharded-slab) |
| shlex | 2.0.1 | MIT OR Apache-2.0 | [source](https://github.com/comex/rust-shlex) |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | [source](https://github.com/vorner/signal-hook) |
| simd-adler32 | 0.3.10 | MIT | [source](https://github.com/mcountryman/simd-adler32) |
| slab | 0.4.12 | MIT | [source](https://github.com/tokio-rs/slab) |
| smallvec | 1.15.2 | MIT OR Apache-2.0 | [source](https://github.com/servo/rust-smallvec) |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | [source](https://github.com/rust-lang/socket2) |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | [source](https://github.com/storyyeller/stable_deref_trait) |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | [source](https://github.com/nvzqz/static-assertions-rs) |
| strsim | 0.11.1 | MIT | [source](https://github.com/rapidfuzz/strsim-rs) |
| subtle | 2.6.1 | BSD-3-Clause | [source](https://github.com/dalek-cryptography/subtle) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/syn) |
| syn | 3.0.3 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/syn) |
| sync_wrapper | 1.0.2 | Apache-2.0 | [source](https://github.com/Actyx/sync_wrapper) |
| synstructure | 0.13.2 | MIT | [source](https://github.com/mystor/synstructure) |
| thiserror | 2.0.20 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/thiserror) |
| thiserror-impl | 2.0.20 | MIT OR Apache-2.0 | [source](https://github.com/dtolnay/thiserror) |
| thread_local | 1.1.10 | MIT OR Apache-2.0 | [source](https://github.com/Amanieu/thread_local-rs) |
| tinystr | 0.8.4 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| tinyvec | 1.12.0 | Zlib OR Apache-2.0 OR MIT | [source](https://github.com/Lokathor/tinyvec) |
| tinyvec_macros | 0.1.1 | MIT OR Apache-2.0 OR Zlib | [source](https://github.com/Soveu/tinyvec_macros) |
| tokio | 1.53.1 | MIT | [source](https://github.com/tokio-rs/tokio) |
| tokio-macros | 2.7.2 | MIT | [source](https://github.com/tokio-rs/tokio) |
| tokio-rustls | 0.26.4 | MIT OR Apache-2.0 | [source](https://github.com/rustls/tokio-rustls) |
| tokio-stream | 0.1.19 | MIT | [source](https://github.com/tokio-rs/tokio) |
| tokio-util | 0.7.19 | MIT | [source](https://github.com/tokio-rs/tokio) |
| toml | 0.9.12+spec-1.1.0 | MIT OR Apache-2.0 | [source](https://github.com/toml-rs/toml) |
| toml_datetime | 0.7.5+spec-1.1.0 | MIT OR Apache-2.0 | [source](https://github.com/toml-rs/toml) |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | [source](https://github.com/toml-rs/toml) |
| toml_edit | 0.25.13+spec-1.1.0 | MIT OR Apache-2.0 | [source](https://github.com/toml-rs/toml) |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | [source](https://github.com/toml-rs/toml) |
| toml_writer | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | [source](https://github.com/toml-rs/toml) |
| tower | 0.5.3 | MIT | [source](https://github.com/tower-rs/tower) |
| tower-http | 0.6.11 | MIT | [source](https://github.com/tower-rs/tower-http) |
| tower-layer | 0.3.3 | MIT | [source](https://github.com/tower-rs/tower) |
| tower-service | 0.3.3 | MIT | [source](https://github.com/tower-rs/tower) |
| tracing | 0.1.44 | MIT | [source](https://github.com/tokio-rs/tracing) |
| tracing-attributes | 0.1.31 | MIT | [source](https://github.com/tokio-rs/tracing) |
| tracing-core | 0.1.36 | MIT | [source](https://github.com/tokio-rs/tracing) |
| tracing-log | 0.2.0 | MIT | [source](https://github.com/tokio-rs/tracing) |
| tracing-subscriber | 0.3.23 | MIT | [source](https://github.com/tokio-rs/tracing) |
| try-lock | 0.2.5 | MIT | [source](https://github.com/seanmonstar/try-lock) |
| typenum | 1.20.1 | MIT OR Apache-2.0 | [source](https://github.com/paholg/typenum) |
| ulid | 1.2.1 | MIT | [source](https://github.com/dylanhart/ulid-rs) |
| unicase | 2.9.0 | MIT OR Apache-2.0 | [source](https://github.com/seanmonstar/unicase) |
| unicode-ident | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | [source](https://github.com/dtolnay/unicode-ident) |
| untrusted | 0.9.0 | ISC | [source](https://github.com/briansmith/untrusted) |
| url | 2.5.8 | MIT OR Apache-2.0 | [source](https://github.com/servo/rust-url) |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | [source](https://github.com/hsivonen/utf8_iter) |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT | [source](https://github.com/alacritty/vte) |
| uuid | 1.25.0 | Apache-2.0 OR MIT | [source](https://github.com/uuid-rs/uuid) |
| vcpkg | 0.2.15 | MIT/Apache-2.0 | [source](https://github.com/mcgoo/vcpkg-rs) |
| version_check | 0.9.5 | MIT/Apache-2.0 | [source](https://github.com/SergioBenitez/version_check) |
| walkdir | 2.5.0 | Unlicense/MIT | [source](https://github.com/BurntSushi/walkdir) |
| want | 0.3.1 | MIT | [source](https://github.com/seanmonstar/want) |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | [source](https://github.com/rustls/webpki-roots) |
| winapi-util | 0.1.11 | Unlicense OR MIT | [source](https://github.com/BurntSushi/winapi-util) |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | [source](https://github.com/microsoft/windows-rs) |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 | [source](https://github.com/microsoft/windows-rs) |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | [source](https://github.com/microsoft/windows-rs) |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 | [source](https://github.com/microsoft/windows-rs) |
| windows_x86_64_msvc | 0.53.1 | MIT OR Apache-2.0 | [source](https://github.com/microsoft/windows-rs) |
| winnow | 0.7.15 | MIT | [source](https://github.com/winnow-rs/winnow) |
| winnow | 1.0.4 | MIT | [source](https://github.com/winnow-rs/winnow) |
| writeable | 0.6.4 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| xdg-home | 1.3.0 | MIT | [source](https://github.com/zeenix/xdg-home) |
| yoke | 0.8.3 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| yoke-derive | 0.8.2 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| zbus | 4.4.0 | MIT | [source](https://github.com/dbus2/zbus/) |
| zbus_macros | 4.4.0 | MIT | [source](https://github.com/dbus2/zbus/) |
| zbus_names | 3.0.0 | MIT | [source](https://github.com/dbus2/zbus/) |
| zerocopy | 0.8.56 | BSD-2-Clause OR Apache-2.0 OR MIT | [source](https://github.com/google/zerocopy) |
| zerofrom | 0.1.8 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| zerofrom-derive | 0.1.7 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| zeroize | 1.9.0 | Apache-2.0 OR MIT | [source](https://github.com/RustCrypto/utils) |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | [source](https://github.com/RustCrypto/utils) |
| zerotrie | 0.2.5 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| zerovec | 0.11.8 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| zerovec-derive | 0.11.6 | Unicode-3.0 | [source](https://github.com/unicode-org/icu4x) |
| zmij | 1.0.23 | MIT | [source](https://github.com/dtolnay/zmij) |
| zvariant | 4.2.0 | MIT | [source](https://github.com/dbus2/zbus/) |
| zvariant_derive | 4.2.0 | MIT | [source](https://github.com/dbus2/zbus/) |
| zvariant_utils | 2.1.0 | MIT | [source](https://github.com/dbus2/zbus/) |

## Embedded console runtime packages (39)

| Package | Version | License | Upstream |
| --- | --- | --- | --- |
| @phosphor-icons/react | 2.1.10 | MIT | [source](https://phosphoricons.com) |
| @radix-ui/primitive | 1.1.7 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-collection | 1.1.15 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-compose-refs | 1.1.5 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-context | 1.2.2 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-direction | 1.1.4 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-id | 1.1.4 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-presence | 1.1.10 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-primitive | 2.1.10 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-radio-group | 1.4.7 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-roving-focus | 1.1.19 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-slot | 1.3.3 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-use-callback-ref | 1.1.4 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-use-controllable-state | 1.2.6 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-use-effect-event | 0.0.5 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-use-is-hydrated | 0.1.3 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-use-layout-effect | 1.1.4 | MIT | [source](https://radix-ui.com/primitives) |
| @radix-ui/react-use-size | 1.1.4 | MIT | [source](https://radix-ui.com/primitives) |
| @tanstack/history | 1.162.1 | MIT | [source](https://tanstack.com/router) |
| @tanstack/query-core | 5.102.0 | MIT | [source](https://tanstack.com/query) |
| @tanstack/react-query | 5.102.0 | MIT | [source](https://tanstack.com/query) |
| @tanstack/react-router | 1.170.32 | MIT | [source](https://tanstack.com/router) |
| @tanstack/react-store | 0.9.3 | MIT | [source](https://tanstack.com/store) |
| @tanstack/react-table | 8.21.3 | MIT | [source](https://tanstack.com/table) |
| @tanstack/router-core | 1.171.27 | MIT | [source](https://tanstack.com/router) |
| @tanstack/store | 0.9.3 | MIT | [source](https://tanstack.com/store) |
| @tanstack/table-core | 8.21.3 | MIT | [source](https://tanstack.com/table) |
| @types/react | 19.2.18 | MIT | [source](https://github.com/DefinitelyTyped/DefinitelyTyped/tree/master/types/react) |
| @types/react-dom | 19.2.4 | MIT | [source](https://github.com/DefinitelyTyped/DefinitelyTyped/tree/master/types/react-dom) |
| clsx | 2.1.1 | MIT | [source](https://github.com/lukeed/clsx#readme) |
| cookie-es | 3.1.1 | MIT | [source](https://github.com/unjs/cookie-es#readme) |
| csstype | 3.2.3 | MIT | [source](https://github.com/frenic/csstype#readme) |
| isbot | 5.2.1 | Unlicense | [source](https://isbot.js.org) |
| react | 19.2.8 | MIT | [source](https://react.dev/) |
| react-dom | 19.2.8 | MIT | [source](https://react.dev/) |
| scheduler | 0.27.0 | MIT | [source](https://react.dev/) |
| seroval | 1.6.3 | MIT | [source](https://github.com/lxsmnsyc/seroval/tree/main/packages/seroval) |
| seroval-plugins | 1.6.3 | MIT | [source](https://github.com/lxsmnsyc/seroval/tree/main/packages/plugins) |
| use-sync-external-store | 1.6.0 | MIT | [source](https://github.com/facebook/react#readme) |
