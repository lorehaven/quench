---
title: Formats
parent: quench-convert
grand_parent: Crates
nav_order: 4
---

## CSV {#csv}

* Lenient by default, in the way most programs read CSV: a quote in the middle of an unquoted field is a character; text after a closing quote joins the field; a quote left open at the end of the text runs to the end of the text. `Options::strict` turns the last two into errors with the line number of the opening quote (or of the closing one).
* Quoting: a field that starts with `"` (after optional spaces when `trim` is on) is quoted; `""` inside is a quote; newlines inside quotes are kept.
* Records end at `\n`, `\r\n` or `\r`; empty records (blank lines) are skipped; a leading BOM is removed.
* With `header`, the header row is consumed. Short records give objects with fewer keys; long ones get `column<N>` keys.
* Examples (real output): `"a,b\n1\n3,4,5\n"` gives `[{"a":"1"},{"a":"3","b":"4","column3":"5"}]`; `"a,,c\n1,2,3,4\n"` gives keys `a`, `column2`, `c`, `column4`.
* Fields are strings; there is no type inference.
* Errors carry a 1-based line (for `flexible = false`, the record number).

## XML {#xml}

Element tree: `{"tag", "attrs", "children"}`; attribute order is document order (`Map` keeps insertion order).

| Input | Result |
|---|---|
| XML declaration `<?xml ?>`, any processing instruction, comments, `<!DOCTYPE ...>` (with an internal subset in brackets) | skipped (before the root, between nodes, after the root) |
| `<![CDATA[x]]>` | text `x`, merged with adjacent text |
| `&lt; &gt; &amp; &quot; &apos;` and `&#NN;` / `&#xHH;` | decoded; any other `&name;` is `unknown entity`; a missing `;` is an error |
| text of only whitespace between elements | dropped unless `keep_whitespace` |
| text mixed with elements | strings and elements in order in `children` |
| tag and attribute names | kept as written, prefix included (`n:s`); characters allowed: alphanumerics and `_ - . :` |
| attribute values | single or double quotes; entities decoded; an unquoted or valueless attribute is an error |
| a leading BOM | removed |
| nesting over 256 | `elements nest too deeply` |

Namespaces are not resolved. Entities defined in a doctype are not expanded (they are unknown entities). `to_string` does not emit a declaration. Writing and re-reading text with leading or trailing spaces keeps them; dropped whitespace-only nodes are lost.

Real example: `<?xml version="1.0"?><!-- c --><r a='1' b="x&amp;y"><i>t</i> <e/><![CDATA[<z>]]>&#65;&#x42;<n:s/></r>` parses to

```json
{"tag":"r","attrs":{"a":"1","b":"x&y"},"children":[{"tag":"i","attrs":{},"children":["t"]},{"tag":"e","attrs":{},"children":[]},"<z>AB",{"tag":"n:s","attrs":{},"children":[]}]}
```

and `to_string_pretty(&doc, 2)` writes

```xml
<r a="1" b="x&amp;y">
  <i>t</i>
  <e/>
  &lt;z&gt;AB
  <n:s/>
</r>
```

## Android binary XML {#axml}

The chunk format of AOSP's `ResXMLTree`: file type `0x0003`, a string pool (`0x0001`, UTF-8 or UTF-16), namespace start/end, element start/end and CDATA chunks. Unknown chunk types are skipped.

* Attribute names carry their namespace prefix (`android:versionCode`) when the namespace has a non-empty prefix in scope.
* Namespace declarations appear as `xmlns:<prefix>` attributes on the **next** element after them (normally the root).
* Values render like `aapt dump xmlstrings`: raw string value if present; else by type: `0x01` reference `@0x7f0b0001`, `0x02` attribute reference `?0x…`, `0x03` string from the pool, `0x04` float, `0x05` dimension (`px dp sp pt in mm` units), `0x06` fraction (`%` or `%p`), `0x10` signed decimal, `0x11` hexadecimal `0x…`, `0x12` boolean `true`/`false`, `0x1c`-`0x1f` colors `#aarrggbb`, `#rrggbb`, `#argb`, `#rgb`; type `0x00` is empty; any other type prints the data word as decimal.
* The text of CDATA chunks becomes string children.

## GGUF {#gguf}

* Layout read: magic `GGUF`, version (1, 2 or 3), tensor count and key/value count (u32 in version 1, u64 from version 2), key/value pairs, then the tensor table when requested. Little-endian only.
* Value types: 0 u8, 1 i8, 2 u16, 3 i16, 4 u32, 5 i32, 6 f32, 7 bool, 8 string, 9 array, 10 u64, 11 i64, 12 f64. Unsigned types become unsigned/int values, floats become `Float`, strings are UTF-8 (lossy).
* Limits: a string over 16 MiB is refused (`a string is implausibly long`); more than 2^20 metadata entries or 2^24 tensors is `implausible entry counts`; a tensor with more than 8 dimensions is refused; arrays nest at most four levels.
* An array longer than `max_array` is still consumed element by element so the stream position stays correct; only the result is summarised.
* `tensor type` is the raw ggml type number; `offset` is relative to the tensor data section, not to the file.
