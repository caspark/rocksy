# Rocksy

A reverse proxy suitable for making one or more web apps running on
several ports appear as if they are run on a single port.

**CURRENTLY IN DEVELOPMENT. NOT SUITABLE FOR USE YET.**

Intended to be used for local development environments (specifically not
expected to be used in production). It's similar to the development proxy
that's built into Webpack, which will forward any requests which do not
match known front-end resources to a configured back-end - except Rocksy
can be used to solve the same problem without having to use Webpack,
which is useful if your front-end doesn't rely on Webpack at all.

## Example usage

Let's say you have:

* a Rust backend running on port 9000 (serves everything at /api)
* the Elm Reactor running on port 8000 (serves front end assets)

And you want to expose these to your browser as one app on port 5555:

    rocksy -p 5555 'backend at http://127.0.0.1:9000 if ^/api.*$' 'frontend at http://127.0.0.1:8000'

Without Rocksy, your front-end would have to make CORS requests to your
backend, which means your backend needs to add CORS headers - but only
in development, which is a pain.

## Installing Rocksy

Rocksy is written in Rust, so at the moment the easiest way is to
compile from source:

    curl https://sh.rustup.rs -sSf | sh # install Rust toolchain
    git clone <rocksy repo> rocksy
    cd rocksy
    cargo install                       # build and install Rocksy
    $HOME/.cargo/bin/rocksy -h

Once installed, you may want to symlink it to `/usr/local/bin/rocksy` or
add `$HOME/.cargo/bin/` to your `$PATH`.

## Removing Rocksy

You can uninstall Rocksy using Cargo too:

    cargo remove rocksy

Don't forget to clean up any symlinks to `$HOME/.cargo/bin/rocksy` if
you created any!

## Known limitations

* No support for proxying web socket connections

## Credit

Heavily based on https://github.com/brendanzab/hyper-reverse-proxy/

## License

Apache.