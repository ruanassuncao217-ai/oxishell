# Contribuindo

- Lógica nova vai em `crates/oxi-core` com testes; a interface só chama o núcleo.
- Qualquer operação que apague ou altere algo precisa ter modo simulação e teste.
- Antes de abrir PR: `cargo test --workspace` e `cargo clippy --workspace --all-targets -- -D warnings`.
