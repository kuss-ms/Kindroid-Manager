import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { renderChatMarkdown } from './chatMarkdown';

describe('renderChatMarkdown', () => {
  function renderToHtml(text: string): string {
    const { container } = render(<>{renderChatMarkdown(text)}</>);
    return container.innerHTML;
  }

  it('returns empty string for empty input', () => {
    expect(renderToHtml('')).toBe('');
  });

  it('returns the raw text when no formatting is used', () => {
    expect(renderToHtml('hello world')).toBe('hello world');
  });

  it('renders balanced italics', () => {
    expect(renderToHtml('*hello*')).toBe('<em>hello</em>');
  });

  it('renders balanced bold', () => {
    expect(renderToHtml('**hello**')).toBe('<strong>hello</strong>');
  });

  it('renders bold before italic (bold wins over the inner *s)', () => {
    expect(renderToHtml('**a *b* c**')).toBe(
      '<strong><span>a </span><em>b</em><span> c</span></strong>',
    );
  });

  it('falls back to raw text on an odd asterisk count', () => {
    // 3 asterisks → odd, can't pair unambiguously.
    const html = renderToHtml('*foo *bar*');
    expect(html).toBe('*foo *bar*');
  });

  it('falls back to raw text on unbalanced bold', () => {
    const html = renderToHtml('**broken');
    expect(html).toBe('**broken');
  });

  it('falls back to raw text on three asterisks in a row', () => {
    const html = renderToHtml('a***b***c');
    expect(html).toBe('a***b***c');
  });

  it('wraps line quotes in .chat-quote', () => {
    const html = renderToHtml('> quoted line');
    expect(html).toBe('<span class="chat-quote">quoted line</span>');
  });

  it('renders inline bold/italic inside a quoted line', () => {
    const html = renderToHtml('> say **bold** here');
    expect(html).toBe(
      '<span class="chat-quote"><span>say </span><strong>bold</strong><span> here</span></span>',
    );
  });

  it('renders multi-line text with mixed quoting and inline', () => {
    const html = renderToHtml('first\n> second\nthird');
    // Newlines are preserved between lines (final line has no trailing \n).
    expect(html).toBe('first\n<span class="chat-quote">second</span>\nthird');
  });

  it('keeps square brackets as literal text', () => {
    expect(renderToHtml('see [this]')).toBe('see [this]');
  });
});
