# Rocksy

A reverse proxy suitable for making multiple web apps running on several
distinct ports appear as if they are run on a single port in a
development environment.

**Status: Initial version is working & ready for use**. If Rocksy is
useful for you, please star the repo as feedback to help me prioritize
my projects.

## Example usage

Let's say you have:

* a Rust backend running on port `9000` (serves everything at `/api`)
* the Elm Reactor running on port `8000` (serves front end assets)

And you want to expose these to your browser as one app on port `5555`:

    rocksy -p 5555 'backend at http://localhost:9000 if ^/api.*$' 'frontend at http://localhost:8000'

## What problem does this solve?

Without Rocksy, your front-end would have to make CORS requests to your
backend, which means your backend needs to add CORS headers - but only
in development, which is a pain.

It's similar to the development proxy that's built into Webpack, which
will forward any requests which do not match known front-end resources
to a configured back-end - except Rocksy can be used to solve the same
problem without having to use Webpack, which is useful if your front-end
doesn't rely on Webpack at all.

Note that Rocksy is intended for 100% development use - for production,
you should use Nginx or similar. (You could also use Nginx to tackle
Rocksy's development-only use case, but Nginx's user experience is less
tuned for that.)

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

## Configuration

Rocksy is configured by specifying one or more "targets" on the
command-line; each target is a server listening on a particular port.

The basic format for a target is the following:

    '<target_name> at <target_base_uri> if <target_path_regex>'

(Note that the single quotes are required to ensure your shell passes
each target as a single argument to Rocksy, rather than splitting each
target into 5 arguments.)

When Rocksy receives a HTTP request on the port that it listens on, it
will proxy the request to the first target whose `target_path_regex`
matches the path of the incoming HTTP request.

For convenience, you can also leave off the `target_name`, the
`target_path_regex`, or both; the most minimal target is simply a URI:

    '<target_base_uri>'

* If you leave off `target_name`, then `target_base_uri` will be used
  as the name. (Be sure to also leave off the text ` at ` - the presence
  of this keyword informs Rocksy that it should parse the `target_name`
  first.)
* If you leave off `target_path_regex`, then this target will match all
  requests. You generally only want this for the final target.
* If you leave off either `target_name` or `target_path_regex`, be sure
  to also leave off the keywords ` at ` or ` if ` (respectively): the
  presence of these keywords tells Rocksy that it should look for the
  other components of a target.

Rocksy also has other command line arguments; for example, it runs on
port `5555` by default, but you can change this with the `-p` flag. Pass
the `--help` flag on the command line for a full listing of available
options.

### Saving configuration to a file

Currently, Rocksy only supports configuration via command-line
arguments. If you'd like to persist configuration to a file, raise an
issue :)

As a workaround, you could define an alias or write a small script to
invoke Rocksy:

```sh
#!/bin/sh

exec $HOME/.cargo/bin/rocksy -p 5555 'backend at http://localhost:9000 if ^/api.*$' 'frontend at http://localhost:8000'
```

## Removing Rocksy

You can uninstall Rocksy using Cargo too:

    cargo remove rocksy

Don't forget to clean up any symlinks to `$HOME/.cargo/bin/rocksy` if
you created any!

## Known limitations

* No support for proxying web socket connections

## Changelog

### v0.1

* initial release

## Credits

The initial wiring together of Tokio and Hyper to proxy requests was
taken from https://github.com/brendanzab/hyper-reverse-proxy/ - thanks
`@brendanzab`!

## License

Apache.