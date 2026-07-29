# PDF Handouts

A cross-platform command-line tool for merging PDFs and adding custom headers and footers.

## Use it in your browser

<https://bridge-craftwork.github.io/pdf-handouts/>

Drop your PDFs and screenshots on the page, fill in the title and footers, and
download the finished handout. It is the same Rust code compiled to
WebAssembly, so it produces the same output as the command line — page content
is byte-for-byte identical.

**Nothing is uploaded.** The whole pipeline runs inside the browser tab, which
matters if your handouts carry student names or anything else you would rather
not put on someone else's server.

That claim is enforced, not just asserted. The page ships a Content Security
Policy with `connect-src 'self'`, which the *browser* applies — the page cannot
switch it off. It blocks `fetch`, `XMLHttpRequest`, `WebSocket`,
`navigator.sendBeacon` and tracking pixels to every other origin.

A page can never prove its own innocence, so the site instead makes it easy to
check independently:

- **Disconnect from the network** once the page has loaded, then build a
  handout anyway. It works. (Verified: with the web server stopped, the page
  still produced a valid PDF and downloaded it.)
- **Watch the Network tab** while building — there are no requests.
- **Try to make it leak** from the developer console and watch the browser
  refuse. The site gives you the snippet.
- **Read or rebuild it.** The front end is dependency-free JavaScript with no
  build step, so the source is what runs; the engine is this Rust crate.

The one honest caveat, stated on the site too: the policy allows requests back
to its own origin, because that is how the WebAssembly module loads. That
origin is GitHub Pages — static hosting with nothing that could accept an
upload. If that is not good enough for your documents, use the CLI, which has
no network code at all.

## Installation

```bash
# Build from source
cargo build --release

# The binary will be at target/release/pdf-handouts
```

## Quick Start

```bash
# Merge multiple PDFs and add headers/footers in one step
pdf-handouts build \
  "1. intro.pdf" "2. main.pdf" "3. appendix.pdf" \
  -o handout.pdf \
  --title "Workshop Handout" \
  --footer-left "Acme Corp" \
  --footer-right "Page [page] of [pages]" \
  --date today
```

## Input formats

Inputs may be PDFs or raster images — **PNG, JPEG, GIF, WebP**. Images are
converted to PDF as they are read, so a folder of handouts can freely mix
screenshots with generated PDFs:

```bash
pdf-handouts build "1. Ladder.pdf" "2. Screenshot.png" "3. Notes.pdf" -o handout.pdf
```

Each image becomes one US Letter page: scaled to fit and centered, on a
landscape page when the image is wider than it is tall. A 1in margin is left on
the two edges that carry the title and footer and 0.5in on the other two — which
on a landscape page means the deeper margin is on the left and right, since
that is where its header and footer go (see [Landscape pages](#landscape-pages)).

The file's type is determined by its contents, not its extension, so a
misnamed file still works. **An input that is neither a PDF nor a supported
image is an error** — it is never silently dropped from the output:

```
Error: Unsupported input file(s):
  5. Notes.txt

Supported formats: PDF, PNG, JPEG, GIF, WebP
```

## Page layout

### Landscape pages

A landscape page — a wide screenshot, say — keeps its landscape shape, so it
still reads correctly on screen and on a projector with no manual rotation.

Its title and footer, though, are drawn turned a quarter turn along the page's
**short edges**. When a printer rotates the landscape page to fit portrait
paper, they land at the top and bottom of the sheet, in line with every other
page in the stack. That keeps a stapled handout consistent: the reader turns
the sheet to look at the picture, but the page furniture is always where they
expect it.

### Content fitting

Source documents know nothing about the title and footer bands, so their
content can collide with them — most visibly, a title printed over the
document's own heading. By default pdf-handouts measures where each page's ink
actually falls and moves it clear:

| Situation | What happens |
|-----------|--------------|
| Content already clears the bands | Left untouched |
| Content merely sits too high or low | **Shifted**; nothing is resized |
| Content too tall to shift | **Scaled** down about the page centre, then centred in the space |

Each page is adjusted on its own, so a page that needs nothing keeps its full
size. The tool reports what it did:

```
Step 2: Adding headers/footers...
  Moved content clear of the title/footer on page(s): 1 (25pt)
```

Control it with `--fit`:

- `auto` — shift when possible, scale when necessary (default)
- `shift` — only ever move content, never resize it
- `off` — leave source content exactly as it is

Fitting is skipped on any page using `--mask-*`. A mask is a promise to cover a
specific region of the source, and moving the content underneath would break it.

## Commands

### `build` - Merge and add headers/footers

The most common workflow: merge multiple PDFs and/or images and add headers/footers in one step.

```bash
pdf-handouts build [OPTIONS] --output <OUTPUT> <INPUTS>...
```

**Arguments:**
- `<INPUTS>...` - Input PDF/image files in order

**Options:**
- `-o, --output <OUTPUT>` - Output PDF file path (required)
- `--title <TITLE>` - Title text (centered at top of first page)
- `--footer-left <TEXT>` - Footer left section
- `--footer-center <TEXT>` - Footer center section
- `--footer-right <TEXT>` - Footer right section
- `--date <DATE>` - Date for `[date]` placeholder
- `--font <SPEC>` - Font specification for both header and footer
- `--header-font <SPEC>` - Font specification for header only
- `--footer-font <SPEC>` - Font specification for footer only
- `--fit <MODE>` - How to keep source content clear of the title/footer: `auto` (default), `shift`, `off`

**Example:**
```bash
pdf-handouts build \
  "lesson1.pdf" "lesson2.pdf" "exercises.pdf" \
  -o "complete-handout.pdf" \
  --title "Bridge Class Handout" \
  --footer-left "Stoneridge Creek|[font italic]Community Center[/font]" \
  --footer-center "Presented by:|Rick Wilson" \
  --footer-right "Page [page] of [pages]|[date]" \
  --date "next tuesday" \
  --font "14pt #333333" \
  --header-font "24pt #222222"
```

### `merge` - Merge only

Merge multiple PDFs and/or images into one PDF without adding headers/footers.

```bash
pdf-handouts merge [OPTIONS] --output <OUTPUT> <INPUTS>...
```

**Example:**
```bash
pdf-handouts merge file1.pdf screenshot.png file3.pdf -o merged.pdf
```

### `headers` - Add headers/footers to existing PDF

Add headers and footers to an already-merged PDF.

```bash
pdf-handouts headers [OPTIONS] --output <OUTPUT> <INPUT>
```

**Example:**
```bash
pdf-handouts headers merged.pdf -o final.pdf \
  --title "My Document" \
  --footer-right "Page [page]"
```

### `info` - Show PDF information

Display page count and metadata for a PDF file.

```bash
pdf-handouts info <INPUT>
```

**Example:**
```bash
pdf-handouts info document.pdf
# Output:
# File: document.pdf
# Pages: 14
# Title: My Document
# Author: John Doe
```

## Text Formatting

### Placeholders

Use these placeholders in footer text - they're replaced with actual values:

| Placeholder | Description |
|-------------|-------------|
| `[page]` | Current page number |
| `[pages]` | Total page count |
| `[date]` | Formatted date (requires `--date`) |

**Example:**
```bash
--footer-right "Page [page] of [pages]|[date]"
# Output: "Page 3 of 14" and "January 14, 2026"
```

### Line Breaks

Use `|` or `[br]` to create multi-line footers:

```bash
--footer-left "Acme Corp|Engineering Division"
# Creates:
#   Acme Corp
#   Engineering Division
```

### Inline Font Styling

Use `[font]...[/font]` tags for inline styling:

| Tag | Effect |
|-----|--------|
| `[font italic]...[/font]` | Italic text |
| `[font bold]...[/font]` | Bold text |
| `[font bold italic]...[/font]` | Bold italic text |

**Example:**
```bash
--footer-left "Company Name|[font italic]Department[/font]"
```

## Font Specification

The `--font`, `--header-font`, and `--footer-font` options accept a font specification string:

```
[bold] [italic] [size[pt]] [family_name] [#rrggbb]
```

All components are optional. Order doesn't matter.

| Component | Description | Example |
|-----------|-------------|---------|
| `bold` | Bold weight | `bold` |
| `italic` | Italic style | `italic` |
| `size` | Font size in points | `14pt` or `14` |
| `family` | Font family (use underscores for spaces) | `Liberation_Serif` |
| `#rrggbb` | Hex color | `#333333` or `#f00` |

**Examples:**
```bash
--font "14pt"                           # 14pt default font
--font "bold 16pt"                      # Bold 16pt
--font "italic 12pt Liberation_Serif"   # Italic 12pt Liberation Serif
--font "24pt #333333"                   # 24pt dark gray
--font "bold italic 18pt #0000ff"       # Bold italic 18pt blue
```

### Font Hierarchy

- `--font` sets the base font for both header and footer
- `--header-font` overrides `--font` for the header only
- `--footer-font` overrides `--font` for the footer only

```bash
pdf-handouts build input.pdf -o output.pdf \
  --font "14pt #333333" \           # Base: 14pt dark gray
  --header-font "24pt #000000"      # Header: 24pt black (overrides base)
```

## Date Expressions

The `--date` option accepts flexible date expressions:

| Expression | Description |
|------------|-------------|
| `today` | Current date |
| `2026-01-14` | ISO format date |
| `01/14/2026` | US format date |
| `Tuesday` | Next Tuesday (or today if Tuesday) |
| `Tuesday+1` | Tuesday after next |
| `Tuesday+3` | 4th upcoming Tuesday |

**Example:**
```bash
--date "next tuesday"    # Next occurrence of Tuesday
--date "2026-01-14"      # Specific date
--date "today"           # Current date
```

## Complete Example

```bash
# Create a workshop handout from multiple source PDFs
pdf-handouts build \
  "1. NT Ladder - Google Docs.pdf" \
  "2. NT Ladder Practice Sheet.pdf" \
  "3. ABS4-2 Jacoby Transfers Handouts.pdf" \
  "4. thinking-bridge-Responding to 1NT 1-6.pdf" \
  -o "Bridge-Workshop-Handout.pdf" \
  --title "Bridge Class Handout" \
  --footer-left "Stoneridge Creek|[font italic]Community Center[/font]" \
  --footer-center "Presented by:|Rick Wilson" \
  --footer-right "Page [page] of [pages]|[date]" \
  --date "next tuesday" \
  --header-font "24pt #333333" \
  --footer-font "14pt #555555"
```

## Library Usage

This tool is also available as a Rust library. See [LIBRARY.md](LIBRARY.md) for API documentation.

Every entry point comes in two forms: a path-based one that reads and writes
files, and a byte-oriented one that does not touch the filesystem at all
(`merge_documents`, `add_headers_footers_bytes`, `build_handout`). The second
set is what the WebAssembly build uses.

## Building the web version

```bash
cd wasm
wasm-pack build --release --target web --out-dir ../web/pkg --no-typescript
cd ../web && python3 -m http.server 8000
```

Then open <http://localhost:8000>. The page must be served over http — browsers
will not load a WebAssembly module from a `file://` URL.

The `wasm` feature turns off the CLI-only dependencies and switches lopdf to a
`getrandom` backend that works on `wasm32-unknown-unknown`. Pushing to `main`
rebuilds and redeploys the site automatically.

## License

MIT OR Apache-2.0
