# OxiShell

Suíte de ferramentas nativas para Windows, escrita em Rust, em **um único executável**: gerenciador de arquivos, busca, gerenciador de tarefas, mapa de uso de disco, limpeza, visualizador de fotos e ajustes de privacidade.

Princípio de design: **nada é destrutivo por padrão.** OxiShell não substitui o Explorer, não esconde a barra de tarefas e não desativa serviços do Windows.

## Ferramentas

| Aba | O que faz |
| --- | --- |
| Arquivos | Navegar, copiar, recortar, colar, renomear, criar pasta. Excluir sempre envia para a **lixeira**. Cópia nunca sobrescreve (usa "nome - copia"). |
| Busca | Indexa uma pasta em segundo plano e busca por nome com ranking (exato > prefixo > trecho > aproximado). Filtros: `ext:pdf`, `dir:`, `file:`. |
| Tarefas | CPU, memória e lista de processos com filtro, ordenação e encerrar. Recusa encerrar o próprio OxiShell. |
| Disco | Varre uma pasta e mostra a árvore de tamanhos com percentuais. Não segue links simbólicos. |
| Limpeza | Remove só **arquivos antigos** de pastas temporárias, depois de mostrar o que seria removido. Nunca remove pastas. |
| Fotos | Visualizador (jpg, png, gif, bmp, webp, tiff) com ordem natural (foto2 antes de foto10) e setas do teclado. |
| Ajustes | Poucos ajustes de privacidade **só do usuário atual** (HKCU), cada um com backup do valor original e botão de desfazer. |

## Linha de comando

```
oxishell                          # abre a interface
oxishell clean [--apply]          # simulação, a menos que use --apply
oxishell tune list
oxishell tune apply <id...|all> [--apply]
oxishell tune undo [id...]
oxishell find <pasta> <consulta>
oxishell disk <pasta> [N]
```

## Compilar

```
cargo build --release -p oxishell      # gera target/release/oxishell.exe
cargo test --workspace
```

Requer Rust estável. A lógica fica em `crates/oxi-core` (portável, com testes); a interface (egui/eframe) e a CLI ficam em `crates/oxishell`.

## Estado do projeto (leia antes de usar)

- A lógica de `oxi-core` tem 25 testes automatizados que passam em Linux.
- O código **específico do Windows** (acesso ao registro nos Ajustes) e a interface gráfica foram escritos com cuidado, mas a verificação em Windows depende do CI (`.github/workflows/ci.yml`, roda em `windows-latest`). Rode o CI no primeiro push e corrija o que aparecer.
- A busca usa varredura de pastas, não a leitura direta da MFT do NTFS; é mais lenta em discos enormes, em troca de ser simples e segura.
- Não há barra de tarefas própria nem substituição do shell, de propósito.
- O executável de release não é assinado digitalmente; o CI publica um SHA-256 junto.

## Ideias futuras

Leitura da MFT (com elevação opcional), pré-visualização de arquivos, abas no gerenciador, assinatura de código.

## Licença

MIT. Veja `LICENSE`.
