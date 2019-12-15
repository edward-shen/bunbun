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

[rust-lang]: https://www.rust-lang.org/
[bunny1]: http://www.bunny1.org/
