lame *args:
  cargo run -p standalone -- {{args}}

build-php-bindings:
  cargo build -p php-bindings --release

install-php-extension:
  cd php-bindings && cargo php install --release --yes

remove-php-extension:
  cd php-bindings && cargo php remove --yes

update-php-extension:
  cd php-bindings && cargo php remove --yes && cargo php install --release --yes

cargo-fmt:
    cargo +nightly fmt