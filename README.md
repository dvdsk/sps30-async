[![Docs](https://docs.rs/sps30/badge.svg)](https://docs.rs/sps30)
[![Version](https://img.shields.io/crates/v/sps30.svg?style=flat-square)](https://crates.io/crates/sps30/)
[![License](https://img.shields.io/crates/l/sps30.svg?style=flat-square)](https://crates.io/crates/sps30/)


## Sensirion SPS30 Particulate Matter Sensor driver

> An async no_std, embedded-io-async UART driver for this device

* [Crate](https://crates.io/crates/sps30)
* [Documentation](https://docs.rs/sps30/)
* [Usage](#usage)
* [License](#license)

This is an opiniated fork of https://github.com/iohe/sps30 replacing the minimal
and reusable non blocking I/O layer [nb](https://github.com/rust-embedded/nb)
with true async. We go through great effort to ensure reliability under 
bad failing connections


## License

Licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
