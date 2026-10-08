**English** | [简体中文](LANGUAGES.zh-CN.md)

# Documentation languages

English is the default language for FarSail documentation and the GitHub About description. Each Markdown page has an English / Simplified Chinese switch at the top.

## Reading

- Open [README](../README.md) for the English introduction.
- The original filename, such as `CLIENT.md`, is the English page.
- The corresponding `CLIENT.zh-CN.md` is the Chinese page.
- A Chinese page's relative document links stay in Chinese; its English switch returns to the same document in English.
- Assets, downloads, source files, external URLs and technical identifiers keep their original targets.

This uses ordinary Markdown links and works without scripts, cookies or browser-language detection. Default repository entry points remain English.

## Maintaining a pair

1. Update the English page and its Chinese counterpart together. Use a full translation, not a summary or a different verification claim.
2. Retain the reciprocal language navigation and original English file path.
3. Keep executable commands, API fields/values, paths, code samples, commit IDs, checksums and recorded test results unchanged. Illustrative diagram labels may be translated while their structure and technical identifiers stay the same.
4. Keep implemented, planned, locally verified, publicly released and unverified states distinct. Historical work-item records describe their recorded version, not a promise about every current build.
5. Retain compatible heading anchors. Translated headings include explicit aliases so existing fragments and links from the other language remain usable.
6. Run the documentation check from the repository root:

   ```sh
   python scripts/check_docs.py
   ```

The check verifies paired files, reciprocal language links, local links, fragments and matching technical content. It does not test application behavior or perform browser automation.

Translations are authored and reviewed in the repository. FarSail's application UI language is independent of this documentation convention.
