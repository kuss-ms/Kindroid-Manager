import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { JournalEntryForm } from './JournalEntryForm';

describe('JournalEntryForm', () => {
  it('shows the "tap Add" hint so Android users see the new affordance', () => {
    render(
      <JournalEntryForm
        initial={{ entry: '', keyphrases: [] }}
        onSubmit={() => {}}
        onCancel={() => {}}
        submitting={false}
      />,
    );
    expect(screen.getByPlaceholderText(/tap Add/)).toBeInTheDocument();
  });

  it('commits a keyphrase when Enter is pressed', () => {
    const onSubmit = vi.fn();
    render(
      <JournalEntryForm
        initial={{ entry: 'hello', keyphrases: [] }}
        onSubmit={onSubmit}
        onCancel={() => {}}
        submitting={false}
      />,
    );
    const input = screen.getByTestId('keyphrase-input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'foo' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(screen.getByText('foo')).toBeInTheDocument();
    expect(input.value).toBe('');
  });

  it('commits a keyphrase when the Add button is clicked (the Android-friendly path)', () => {
    const onSubmit = vi.fn();
    render(
      <JournalEntryForm
        initial={{ entry: 'hello', keyphrases: [] }}
        onSubmit={onSubmit}
        onCancel={() => {}}
        submitting={false}
      />,
    );
    const input = screen.getByTestId('keyphrase-input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'bar' } });
    fireEvent.click(screen.getByTestId('keyphrase-add'));
    expect(screen.getByText('bar')).toBeInTheDocument();
    expect(input.value).toBe('');
  });

  it('flushes the pending keyphrase on blur (the Android "Next" arrow scenario)', () => {
    // The Android virtual keyboard's "Next" button moves focus without
    // firing keydown for Enter. The onBlur handler is what catches that.
    render(
      <JournalEntryForm
        initial={{ entry: 'hello', keyphrases: [] }}
        onSubmit={() => {}}
        onCancel={() => {}}
        submitting={false}
      />,
    );
    const input = screen.getByTestId('keyphrase-input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'pending' } });
    fireEvent.blur(input);
    expect(screen.getByText('pending')).toBeInTheDocument();
    expect(input.value).toBe('');
  });

  it('dedupes case-insensitively and clears the input even when no new chip is added', () => {
    render(
      <JournalEntryForm
        initial={{ entry: 'hello', keyphrases: ['Foo'] }}
        onSubmit={() => {}}
        onCancel={() => {}}
        submitting={false}
      />,
    );
    const input = screen.getByTestId('keyphrase-input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'foo' } });
    fireEvent.click(screen.getByTestId('keyphrase-add'));
    // The original "Foo" chip is still there and the input is cleared.
    expect(screen.getByText('Foo')).toBeInTheDocument();
    expect(input.value).toBe('');
  });

  it('Backspace on an empty input removes the last chip', () => {
    render(
      <JournalEntryForm
        initial={{ entry: 'hello', keyphrases: ['a', 'b'] }}
        onSubmit={() => {}}
        onCancel={() => {}}
        submitting={false}
      />,
    );
    const input = screen.getByTestId('keyphrase-input') as HTMLInputElement;
    fireEvent.keyDown(input, { key: 'Backspace' });
    expect(screen.queryByText('b')).not.toBeInTheDocument();
    expect(screen.getByText('a')).toBeInTheDocument();
  });

  it('disables the input and Add button at the 8-keyphrase cap', () => {
    render(
      <JournalEntryForm
        initial={{ entry: 'hello', keyphrases: ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'] }}
        onSubmit={() => {}}
        onCancel={() => {}}
        submitting={false}
      />,
    );
    expect(screen.getByTestId('keyphrase-input')).toBeDisabled();
    expect(screen.getByTestId('keyphrase-add')).toBeDisabled();
  });

  it('submits the entry text and the current keyphrase list', () => {
    const onSubmit = vi.fn();
    render(
      <JournalEntryForm
        initial={{ entry: '', keyphrases: [] }}
        onSubmit={onSubmit}
        onCancel={() => {}}
        submitting={false}
      />,
    );
    const textarea = screen.getByPlaceholderText(/Write the journal entry/) as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: 'my entry' } });

    const input = screen.getByTestId('keyphrase-input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'k1' } });
    fireEvent.click(screen.getByTestId('keyphrase-add'));
    fireEvent.change(input, { target: { value: 'k2' } });
    fireEvent.click(screen.getByTestId('keyphrase-add'));

    fireEvent.click(screen.getByText('Save entry'));
    expect(onSubmit).toHaveBeenCalledWith({
      id: undefined,
      entry: 'my entry',
      keyphrases: ['k1', 'k2'],
    });
  });

  it('sets enterKeyHint="done" so the Android keyboard labels its action "Done"', () => {
    render(
      <JournalEntryForm
        initial={{ entry: '', keyphrases: [] }}
        onSubmit={() => {}}
        onCancel={() => {}}
        submitting={false}
      />,
    );
    expect(screen.getByTestId('keyphrase-input')).toHaveAttribute('enterkeyhint', 'done');
  });
});
