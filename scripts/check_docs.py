#!/usr/bin/env python3
"""Check English-first Markdown pairs and local navigation without a browser."""

from __future__ import annotations

import argparse
from collections import Counter
from pathlib import Path
import re
import subprocess
import textwrap
from urllib.parse import unquote, urlsplit

FENCE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")
LINK = re.compile(r'!?\[[^\]\n]*\]\(\s*(<[^>]*>|[^\s)]+)(?:\s+[\"\'][^\"\']*[\"\'])?\s*\)')
INLINE = re.compile(r"(?<!`)(`+)(.+?)\1(?!`)", re.S)
HEX = re.compile(r"(?:[a-f0-9]{64}|[a-f0-9]{40})(?![a-f0-9])", re.I)


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8-sig").replace("\r\n", "\n")


def chinese_path(path: Path) -> Path:
    return path.with_name(path.stem + ".zh-CN.md")


def split_blocks(content: str) -> tuple[str, list[tuple[str, str]]]:
    prose, blocks, body = [], [], []
    marker = None
    info = ""
    for line in content.splitlines():
        match = FENCE.match(line)
        if marker is None and match:
            marker, info = match.group(1), match.group(2).strip()
            body = []
            prose.append("")
        elif marker and match and not match.group(2).strip() and match.group(1)[0] == marker[0] and len(match.group(1)) >= len(marker):
            blocks.append((info, textwrap.dedent("\n".join(body))))
            marker = None
            prose.append("")
        elif marker:
            body.append(line)
            prose.append("")
        else:
            prose.append(line)
    if marker:
        raise ValueError("unclosed fenced block")
    return "\n".join(prose), blocks


def slug(text: str) -> str:
    text = re.sub(r"<[^>]+>", "", text).replace("`", "")
    text = re.sub(r"\[([^]]+)\]\([^)]*\)", r"\1", text)
    return re.sub(r"[^\w\- ]", "", text.lower()).replace(" ", "-")


def headings(content: str) -> list[tuple[int, int, str, str]]:
    prose, _ = split_blocks(content)
    seen: dict[str, int] = {}
    result = []
    for line_number, line in enumerate(prose.splitlines()):
        match = re.match(r"^(#{1,6})\s+(.+?)\s*#*\s*$", line)
        if not match:
            continue
        base = slug(match.group(2))
        count = seen.get(base, 0)
        seen[base] = count + 1
        result.append((line_number, len(match.group(1)), match.group(2), base + (f"-{count}" if count else "")))
    return result


def anchors(content: str) -> set[str]:
    prose, _ = split_blocks(content)
    return {entry[3] for entry in headings(content)} | set(re.findall(r'<a\s+[^>]*id=["\']([^"\']+)', prose))


def technical_literals(content: str) -> list[str]:
    prose, _ = split_blocks(content)
    # GFM tables require escaping a pipe even inside a code span. The table
    # parser removes that escape before displaying the technical literal.
    return [match.group(2).replace("\\|", "|") for match in INLINE.finditer(prose)]


def numeric_evidence(content: str) -> Counter[str]:
    prose, _ = split_blocks(content)
    prose = re.sub(r"^.*\*\*(?:English|简体中文)\*\*.*\n?", "", prose, count=1)
    prose = INLINE.sub("", prose)
    prose = re.sub(r"</?(?:a|img|p|br|details|summary|div|span|b|i|strong|em)\b[^>]*>", "", prose, flags=re.I)
    prose = HEX.sub("", prose)
    prose = LINK.sub(lambda match: match.group(0).split("](")[0] + "]", prose)
    prose = re.sub(r"^\s*\d+\.\s+", "", prose, flags=re.M)
    values = []
    for token in re.findall(r"\d+(?:[.,]\d+)*", prose):
        value = token.replace(",", "")
        values.append(str(int(value)) if value.isdigit() else value)
    return Counter(values)


def link_targets(content: str) -> list[str]:
    prose, _ = split_blocks(content)
    prose = INLINE.sub("", prose)
    targets = [m.group(1).strip("<>") for m in LINK.finditer(prose)]
    targets += re.findall(r'<(?:img|a)\b[^>]*(?:src|href)=["\']([^"\']+)', prose)
    targets += re.findall(r'^\[[^]]+\]:\s*(\S+)', prose, re.M)
    return targets


def illustration_signature(info: str, body: str) -> str:
    if info == "mermaid":
        body = re.sub(r"\[([^]]*)\]", lambda m: "[" + "".join(c for c in m.group(1) if c in "()[]") + "]", body)
        body = re.sub(r"\|[^|]*\|", "||", body)
        return re.sub(r"\s+", "", body)
    paths = re.findall(r"^([a-z-]+(?:/[a-z-]+)*/)", body, re.M)
    terms = ["Tauri", "TypeScript", "Android", "iOS", "DXGI", "Media Foundation", "iroh", "Kotlin/Swift", "iroh-relay", "Zstd/LZ4"]
    return repr((paths, [(term, body.count(term)) for term in terms], body.count("→"), body.count("←")))


def blocks_match(path: Path, a: str, b: str) -> bool:
    _, left = split_blocks(a)
    _, right = split_blocks(b)
    if len(left) != len(right):
        return False
    for (info, body), (other_info, other_body) in zip(left, right):
        if info != other_info:
            return False
        if path.name == "PROJECT_DESIGN.md" and info in {"mermaid", "text"}:
            if illustration_signature(info, body) != illustration_signature(info, other_body):
                return False
        elif body != other_body:
            return False
    return True


def inventory(root: Path) -> list[Path]:
    result = subprocess.run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "*.md"], cwd=root, check=True, capture_output=True)
    return sorted({Path(name) for name in result.stdout.decode("utf-8").split("\0") if name})


def check(root: Path, snapshot: Path | None = None) -> tuple[list[str], int]:
    if snapshot is not None and not snapshot.is_dir():
        return [f"Snapshot directory does not exist: {snapshot}"], 0
    errors = []
    paths = inventory(root)
    english = [path for path in paths if not path.name.endswith(".zh-CN.md")]
    for path in paths:
        if path.name.endswith(".zh-CN.md") and path.with_name(path.name.replace(".zh-CN.md", ".md")) not in english:
            errors.append(f"{path}: missing English original")
    for path in english:
        zh = chinese_path(path)
        if not (root / zh).is_file():
            errors.append(f"{path}: missing Chinese counterpart")
            continue
        en_content, zh_content = read(root / path), read(root / zh)
        if en_content.splitlines()[0] != f"**English** | [简体中文]({zh.name})":
            errors.append(f"{path}: English-default switch missing")
        if zh_content.splitlines()[0] != f"[English]({path.name}) | **简体中文**":
            errors.append(f"{zh}: reciprocal switch missing")
        if Counter(technical_literals(en_content)) != Counter(technical_literals(zh_content)):
            errors.append(f"{path}: bilingual inline literals differ")
        if Counter(HEX.findall(en_content)) != Counter(HEX.findall(zh_content)):
            errors.append(f"{path}: bilingual commit/checksum values differ")
        if numeric_evidence(en_content) - numeric_evidence(zh_content):
            errors.append(f"{path}: English numeric evidence missing from Chinese")
        if not blocks_match(path, en_content, zh_content):
            errors.append(f"{path}: bilingual code/illustration structure differs")
        if [h[1] for h in headings(en_content)] != [h[1] for h in headings(zh_content)]:
            errors.append(f"{path}: bilingual section hierarchy differs")
        if snapshot and (snapshot / path).is_file():
            source = read(snapshot / path)
            for target in (en_content, zh_content):
                source_levels = [entry[1] for entry in headings(source)]
                target_levels = [entry[1] for entry in headings(target)]
                if path.as_posix() == "README.md":
                    if len(target_levels) < len(source_levels):
                        errors.append(f"{path}: original README sections lost")
                elif source_levels != target_levels:
                    errors.append(f"{path}: original section hierarchy changed")
                if numeric_evidence(source) - numeric_evidence(target):
                    errors.append(f"{path}: original numeric evidence lost or changed")
                if not blocks_match(path, source, target):
                    errors.append(f"{path}: original code/illustration content changed")
                if Counter(technical_literals(source)) - Counter(technical_literals(target)):
                    errors.append(f"{path}: original inline literals lost")
                if Counter(HEX.findall(source)) - Counter(HEX.findall(target)):
                    errors.append(f"{path}: original commit/checksum values lost")
    for path in paths:
        content = read(root / path)
        for target in link_targets(content):
            parsed = urlsplit(target)
            if parsed.scheme or parsed.netloc:
                continue
            destination = (root / path.parent / unquote(parsed.path)).resolve() if parsed.path else (root / path).resolve()
            if not destination.is_relative_to(root):
                errors.append(f"{path}: local link escapes repository: {target}")
            elif not destination.exists():
                errors.append(f"{path}: local link missing: {target}")
            elif parsed.fragment and destination.is_file() and destination.suffix == ".md" and unquote(parsed.fragment) not in anchors(read(destination)):
                errors.append(f"{path}: heading fragment missing: {target}")
        if path.name.endswith(".zh-CN.md"):
            for target in link_targets("\n".join(content.splitlines()[1:])):
                parsed = urlsplit(target)
                if not parsed.scheme and not parsed.netloc and parsed.path.endswith(".md") and not Path(parsed.path).name.endswith(".zh-CN.md"):
                    errors.append(f"{path}: Chinese document link leaves locale: {target}")
    return errors, len(english)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", type=Path, help="Optional pre-translation Markdown snapshot.")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    snapshot = args.snapshot.resolve() if args.snapshot else None
    if snapshot and not snapshot.is_dir():
        print(f"Snapshot directory does not exist: {snapshot}")
        return 1
    errors, count = check(root, snapshot)
    if errors:
        for error in errors:
            print(error)
        return 1
    print(f"Documentation checks passed: {count} English/Chinese pairs; reciprocal switches, links, anchors and technical content.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
