# https://just.systems

default:
    just --list

# Check if the crate is installed and update it if necessary
@check-cargo crate:
    #!/usr/bin/env bash

    current_version=$(cargo install --list | grep "{{crate}} " | awk '{print $2}' | tr -d 'v:')
    latest_version=$(cargo search {{crate}} --limit 1 | grep "{{crate}} " | awk -F\" '{print $2}')

    if [ "$current_version" != "$latest_version" ]; then
        cargo install --force {{crate}} >/dev/null 2>&1
        printf "Updated %s from %s to %s\n" "{{crate}}" "$current_version" "$latest_version"
    fi


# Format source code
@fmt:
    cargo fmt --all
    cargo fix --workspace --all-targets --allow-dirty --allow-staged
    cargo clippy --workspace --all-targets --allow-dirty --allow-staged --fix

# Run tests
@test:
    RUST_BACKTRACE=1 cargo test --locked --workspace -- --nocapture