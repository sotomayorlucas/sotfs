# sotFS — el libro

Libro técnico standalone sobre el sistema sotFS, en español.

## Compilar

Requiere LuaLaTeX y biber (Fedora: `dnf install texlive-scheme-full
biber`).

```sh
just book          # → docs/book/build/main.pdf
just book-watch    # latexmk -pvc, recompila al guardar
just book-clean    # limpia auxiliares
```

O directamente:

```sh
cd docs/book
latexmk -lualatex -outdir=build main.tex
```

## Estructura

- `main.tex` — entry point.
- `sotfsbook.cls` — clase derivada de `book` que carga `preamble`, `envs`, `macros`, `colors`.
- `chapters/` — los 14 capítulos.
- `appendices/` — apéndices A–E.
- `frontmatter/` — tapa, copyright, prefacio, notación, índices.
- `figs/` — diagramas TikZ standalone reutilizables.
- `refs.bib` — bibliografía BibLaTeX.

## Estado

Trabajo en progreso. Ver `chapters/cap-XX-*.tex` para el estado de cada
capítulo.
