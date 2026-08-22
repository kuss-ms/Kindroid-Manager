import { Fragment, type ReactNode } from 'react';

/**
 * Tolerant in-house markdown renderer for chat bubbles. Supports:
 *   - `**bold**` (rendered first to keep the inner `*…*` scan simple)
 *   - `*italic*`
 *   - `> …` line quote (wraps the line in a `<span class="chat-quote">`)
 *   - `[literal]` (square brackets are kept as-is — no link parsing)
 *
 * Ambiguity policy:
 *   - An odd total number of `*` characters is ambiguous (`*foo *bar* baz*`
 *     has 5), so the renderer falls back to the raw text instead of
 *     guessing where the matching delimiter is. Auto-closing would hide
 *     user errors.
 *   - Unbalanced `**` (one `**` with no closing pair, or three `*` in a
 *     row) also falls back to raw text.
 *
 * Quotes are line-level: each line is rendered independently. A `>`
 * prefix wraps the whole line (including any inline bold/italic in the
 * rest of the line) in the quote span.
 */
export function renderChatMarkdown(text: string): ReactNode {
  if (text == null || text === '') return text;

  const starCount = countChar(text, '*');
  if (starCount % 2 !== 0) {
    return renderLinesAsRaw(text);
  }

  // Try the rich path. splitBold / splitItalic return null when the
  // delimiters don't pair up; we then fall back to the raw text path.
  try {
    const rendered = renderRich(text);
    if (rendered === null) return renderLinesAsRaw(text);
    return rendered;
  } catch {
    return renderLinesAsRaw(text);
  }
}

type InlineSegment = { kind: 'plain'; text: string } | { kind: 'bold'; text: string };

type ItalicSegment = { kind: 'plain'; text: string } | { kind: 'italic'; text: string };

function renderRich(text: string): ReactNode | null {
  const lines = text.split('\n');
  const out: ReactNode[] = [];
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const parsed = parseQuotedLine(line);
    const inline = renderInline(parsed.body);
    if (inline === null) return null;
    out.push(
      <Fragment key={i}>
        {parsed.quoted ? <span className="chat-quote">{inline}</span> : inline}
        {i < lines.length - 1 ? '\n' : null}
      </Fragment>,
    );
  }
  return out;
}

function parseQuotedLine(line: string): { quoted: boolean; body: string } {
  if (line.startsWith('> ')) return { quoted: true, body: line.slice(2) };
  if (line.startsWith('>')) return { quoted: true, body: line.slice(1) };
  return { quoted: false, body: line };
}

function renderLinesAsRaw(text: string): ReactNode {
  const lines = text.split('\n');
  return (
    <>
      {lines.map((line, i) => {
        const parsed = parseQuotedLine(line);
        return (
          <Fragment key={i}>
            {parsed.quoted ? <span className="chat-quote">{parsed.body}</span> : parsed.body}
            {i < lines.length - 1 ? '\n' : null}
          </Fragment>
        );
      })}
    </>
  );
}

/**
 * Render bold first, then italic, then plain. Returns a flat ReactNode
 * list (or the original string when no formatting matched and there's
 * only one plain segment with no asterisks — keeps the DOM shallow).
 */
function renderInline(text: string): ReactNode | null {
  const segments = splitBold(text);
  if (segments === null) return null;
  // Fast path: a single plain segment with no remaining `*` chars
  // (bold already handled) is safe to render as a plain string. If
  // it still has asterisks we have to run the italic pass to pair them.
  if (segments.length === 1 && segments[0].kind === 'plain') {
    const t = segments[0].text;
    if (!t.includes('*')) return t;
  }
  const out: ReactNode[] = [];
  for (let i = 0; i < segments.length; i += 1) {
    const seg = segments[i];
    if (seg.kind === 'bold') {
      out.push(<strong key={`b${i}`}>{renderInline(seg.text)}</strong>);
      continue;
    }
    const italicParts = splitItalic(seg.text);
    if (italicParts === null) return null;
    for (let j = 0; j < italicParts.length; j += 1) {
      const part = italicParts[j];
      if (part.kind === 'italic') {
        out.push(<em key={`i${i}-${j}`}>{part.text}</em>);
      } else {
        out.push(<span key={`p${i}-${j}`}>{part.text}</span>);
      }
    }
  }
  return out;
}

/**
 * Split `text` on `**…**` pairs. Returns `null` if any `**` is unmatched
 * (one with no closing pair, or three `*` in a row).
 */
function splitBold(text: string): InlineSegment[] | null {
  const out: InlineSegment[] = [];
  let i = 0;
  let buf = '';
  while (i < text.length) {
    if (text[i] === '*' && text[i + 1] === '*') {
      if (text[i + 2] === '*') {
        // Three in a row → ambiguous.
        return null;
      }
      if (buf) {
        out.push({ kind: 'plain', text: buf });
        buf = '';
      }
      const close = text.indexOf('**', i + 2);
      if (close === -1) return null;
      out.push({ kind: 'bold', text: text.slice(i + 2, close) });
      i = close + 2;
    } else {
      buf += text[i];
      i += 1;
    }
  }
  if (buf) out.push({ kind: 'plain', text: buf });
  return out;
}

/**
 * Split `text` on `*…*` pairs. Returns `null` if a `*` is unmatched.
 * Assumes the caller has already stripped `**…**` so `*` runs are
 * guaranteed to be pairs.
 */
function splitItalic(text: string): ItalicSegment[] | null {
  const out: ItalicSegment[] = [];
  let i = 0;
  let buf = '';
  while (i < text.length) {
    if (text[i] === '*') {
      if (buf) {
        out.push({ kind: 'plain', text: buf });
        buf = '';
      }
      const close = text.indexOf('*', i + 1);
      if (close === -1) return null;
      out.push({ kind: 'italic', text: text.slice(i + 1, close) });
      i = close + 1;
    } else {
      buf += text[i];
      i += 1;
    }
  }
  if (buf) out.push({ kind: 'plain', text: buf });
  return out;
}

function countChar(text: string, ch: string): number {
  let n = 0;
  for (let i = 0; i < text.length; i += 1) if (text[i] === ch) n += 1;
  return n;
}
