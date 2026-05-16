\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{sotfsbook}[2026/05/13 sotFS standalone book class]

% Pasa todas las opciones a book (luego podemos sumar custom)
\DeclareOption*{\PassOptionsToClass{\CurrentOption}{book}}
\ProcessOptions\relax

\LoadClass[11pt, a4paper, openany, twoside]{book}

% Cargar nuestras hojas en orden:
\RequirePackage{colors}
\RequirePackage{preamble}
\RequirePackage{envs}
\RequirePackage{macros}
