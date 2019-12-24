# bunbun

_Self-hostable, easy-to-configure, fast search/jump multiplexer service._

bunbun is a pure-[Rust][rust-lang] implementation of [bunny1][bunny1], providing
a customizable search engine and quick-jump tool in one.

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

## Installation

If you have `cargo`, you can simply run `cargo install bunbun`.

Once installed, simply run it. A default config file will be created if one does
not exist. You should model your own custom routes after the provided ones.

### Configuration

If configuring for development, no further configuration is required. If running
this for production, you should edit the `public_address` field.

the config file is watched, so updates are immediate unless invalid, or if
you're using certain programs such as nvim, which performs updating a file via
swapping rather than directly updating the file.

### Adding bunbun as a search engine

bunbun supports the [OpenSearch Description Format][osdf]. Visit the root page
of your desired instance of bunbun to learn more.

## Reasons to use bunbun

- Portable: bunbun runs off a single binary and config file.
- Small: binary is 1.3MB (after running `strip` and `upx --lzma` on the release
  binary).
- Convenient: bunbun watches for config changes and refreshes its routes
  automatically, allowing for rapid development.
- Memory-safe: Built with [Rust][rust-lang].

[rust-lang]: https://www.rust-lang.org/
[bunny1]: http://www.bunny1.org/
[osdf]: https://developer.mozilla.org/en-US/docs/Web/OpenSearch
