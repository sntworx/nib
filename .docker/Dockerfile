# Dev container for building bindings-php (the ext-php-rs extension) and
# running PHP scripts against it.
#
# Base is the official PHP CLI image rather than a plain Debian/Rust image:
# it already ships phpize/php-config and the PHPIZE_DEPS build toolchain
# (gcc, make, autoconf, pkg-config, ...) needed to compile a PHP extension,
# which is exactly what ext-php-rs's build.rs shells out to. We only need to
# add Rust itself plus clang/libclang, which bindgen (pulled in by
# ext-php-rs) needs to parse the Zend headers.
ARG PHP_VERSION=8.3
FROM php:${PHP_VERSION}-cli-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    clang \
    libclang-dev \
    git \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# The base image ships php.ini-development/-production templates but no
# active php.ini — cargo-php needs a real one to add the extension= line to.
RUN cp /usr/local/etc/php/php.ini-development /usr/local/etc/php/php.ini

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# cargo-php links against ext-php-rs's "runtime" feature, which still leaves
# a handful of Zend symbols (e.g. zend_hash_del) as unresolved dynamic
# symbols in the cargo-php binary itself rather than binding them lazily —
# without libphp.so preloaded so the dynamic linker can satisfy them at
# startup, every `cargo php ...` invocation dies with a "symbol lookup
# error" before it even parses argv.
ENV LD_PRELOAD=/usr/local/lib/libphp.so

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --profile minimal --default-toolchain stable && \
    rustc --version && cargo --version

# cargo-php: installs/enables the built extension into this container's PHP.
# just: convenience wrapper around the justfile recipes at the repo root.
RUN cargo install cargo-php just

WORKDIR /app

# Usage (from the .docker/ directory):
#   docker compose run --rm nib-php-dev
# then, inside the container, once per container instance (install writes
# to /usr/local/etc/php and the extension_dir, which live in the container
# filesystem, not the /app bind mount, so they don't persist across runs):
#   just php-extension-install
#   php your_script.php
CMD ["bash"]
