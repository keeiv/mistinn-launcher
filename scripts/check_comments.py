"""Comment style check, run by CI (see CONTRIBUTING.md). Prints every problem and exits 1 if any.

Rust (src-tauri):
  - every file opens with a `//!` module comment;
  - every item at module level (fn, struct, enum, trait, type, const, static) and every `pub fn`
    inside an impl has a `///` doc comment; test modules are exempt;
  - no `/* */` block comments: use `//`, `///` or `//!`.
JavaScript (src/*.js):
  - the file opens with a comment saying what it is;
  - every top-level function has a comment directly above it.
CSS (src/*.css): the file opens with a comment.
Everywhere: a line comment has a space after its slashes, and no TODO, FIXME, XXX or HACK is left
in a comment.

    python scripts/check_comments.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEFTOVER = re.compile(r"\b(TODO|FIXME|XXX|HACK)\b")
RUST_ITEM = re.compile(r"^(pub(\([^)]*\))? )?(const fn |async fn |unsafe fn |fn |struct |enum |trait |type |const |static )")
RUST_METHOD = re.compile(r"^    pub (const |async )?fn ")
JS_FUNCTION = re.compile(r"^(export )?(async )?function ")


def line_comment(line: str) -> str | None:
    """The `//` comment on a line, if the line is one (not a URL or a string)."""
    stripped = line.lstrip()
    return stripped if stripped.startswith("//") else None


def common(path: Path, lines: list[str], problems: list[str]) -> None:
    """Rules for every language: a space after `//`, and nothing left to do later."""
    for number, line in enumerate(lines, 1):
        comment = line_comment(line)
        if comment is not None:
            body = comment.lstrip("/!")
            if body and not body.startswith(" "):
                problems.append(f"{path}:{number}: a line comment has a space after its slashes")
        if (comment is not None or "/*" in line or line.lstrip().startswith("*")) and LEFTOVER.search(line):
            problems.append(f"{path}:{number}: no TODO, FIXME, XXX or HACK left in a comment")


def documented(lines: list[str], index: int, doc: str) -> bool:
    """Whether the item on line `index` has a doc comment above it (attributes and `//` notes may sit between)."""
    i = index - 1
    while i >= 0:
        text = lines[i].strip()
        if text.startswith(doc):
            return True
        if text.startswith("#[") or text.startswith("#![") or (text.startswith("//") and not text.startswith("///")):
            i -= 1
            continue
        return False
    return False


def rust(path: Path, problems: list[str]) -> None:
    """Module comment, item docs and no block comments."""
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or not lines[0].startswith("//!"):
        problems.append(f"{path}:1: a Rust file opens with a `//!` module comment")
    in_tests = False
    for index, line in enumerate(lines):
        if line.startswith("#[cfg(test)]"):
            in_tests = True
        if in_tests:
            continue
        if "/*" in line and not line.lstrip().startswith("//"):
            problems.append(f"{path}:{index + 1}: no block comments in Rust: use //, /// or //!")
        if RUST_ITEM.match(line) or RUST_METHOD.match(line):
            if not documented(lines, index, "///"):
                problems.append(f"{path}:{index + 1}: `{line.strip()[:60]}` needs a /// doc comment")
    common(path, lines, problems)


def javascript(path: Path, problems: list[str]) -> None:
    """File comment and a comment above every top-level function."""
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or not (lines[0].startswith("//") or lines[0].startswith("/*")):
        problems.append(f"{path}:1: the file opens with a comment saying what it is")
    for index, line in enumerate(lines):
        if JS_FUNCTION.match(line):
            above = lines[index - 1].strip() if index else ""
            if not (above.startswith("//") or above.endswith("*/")):
                problems.append(f"{path}:{index + 1}: `{line.strip()[:60]}` needs a comment above it")
    common(path, lines, problems)


def css(path: Path, problems: list[str]) -> None:
    """File comment."""
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or not lines[0].startswith("/*"):
        problems.append(f"{path}:1: the file opens with a comment saying what it is")
    common(path, lines, problems)


def main() -> int:
    """Checks every source file and reports what breaks the rules."""
    problems: list[str] = []
    for path in sorted((ROOT / "src-tauri").glob("**/*.rs")):
        if "target" not in path.parts:
            rust(path, problems)
    for path in sorted((ROOT / "src").glob("*.js")):
        javascript(path, problems)
    for path in sorted((ROOT / "src").glob("*.css")):
        css(path, problems)
    for problem in problems:
        print(problem.replace(str(ROOT) + "\\", "").replace(str(ROOT) + "/", ""))
    print(f"{len(problems)} comment style problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
