# bunbun

_Self-hostable, easy-to-configure, fast search/jump multiplexer service._

bunbun is a pure-[Rust][rust-lang] implementation of [bunny1][bunny1], providing
a customizable search engine and quick-jump tool in one small binary.

After adding it to your web-browser and setting it as your default search
engine, you'll gain the ability to quick-jump to a specific page or search from
a specific engine:

```
g hello world   // Searches "hello world" in google
r anime         // Goes to reddit.com/r/anime
ls              // Lists all available commands and aliases
foo bar         // If foo is a defined command, do something with bar
                // Alternatively, if a default route is set, use the entire
                // query for the default route
```

## Reasons to use bunbun

- Convenient: bunbun watches for config changes and refreshes its routes
  automatically, allowing for rapid development.
- Extensible: supports simple route substitution or execution of arbitrary
  programs for complex route resolution.
- Portable: bunbun runs off a single binary and config file.
- Small: binary is 1.3MB (after running `strip` and `upx --lzma` on the release
  binary).
- Memory-safe: Built with [Rust][rust-lang].

## Installation

If you have `cargo`, you can simply run `cargo install bunbun`.

Once installed, simply run it. A default config file will be created if one does
not exist.

If you're looking to run this as a daemon (as most would do), you should put the
binary in `/usr/bin` and copy `aux/systemd/bunbun.service` into your preferred
systemd system folder. Then you may run `systemctl enable bunbun --now` to start
a daemon instance of bunbun.

If running Arch Linux, you may use the provided `PKGBUILD` to install bunbun.
Run `makepkg` followed by `sudo pacman -U bunbun.<version>.tar.gz`. This
installs the systemd service for you. Run `systemctl enable bunbun --now` to
start bunbun as a daemon.

### Building for production

If you're looking to build a release binary, here are the steps I use:

1. `cargo build --release`
2. `strip target/release/bunbun`
3. `upx --lzma target/release/bunbun`

LZMA provides the best level of compress for Rust binaries; it performs at the
same level as `upx --ultra-brute` without the time cost and [without breaking
the binary](https://github.com/upx/upx/issues/224).

### Configuration

If configuring for development, no further configuration is required. If running
this for production, you should edit the `public_address` field.

the config file is watched, so updates are immediate unless invalid, or if
you're using certain programs such as `nvim`, which performs updating a file via
swapping rather than directly updating the file.

### Adding bunbun as a search engine

bunbun supports the [OpenSearch Description Format][osdf]. Visit the root page
of your desired instance of bunbun to learn more.

[rust-lang]: https://www.rust-lang.org/
[bunny1]: http://www.bunny1.org/
[osdf]: https://developer.mozilla.org/en-US/docs/Web/OpenSearch
