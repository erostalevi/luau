// Create card from clipboard (as is / AI review).
export default {
  clipboard: {
    how: 'How should the clipboard become cards?',
    asIs: 'As is',
    asIsDesc: 'One card: the first line is the title, the rest stays as it is',
    aiReview: 'Let AI review it',
    aiReviewDesc: 'AI splits it into cards with titles, tasks, tags and properties',
    reviewing: 'AI is reading the text…',
    aiFailed: 'AI could not create cards: {message}',
    noCards: 'no cards were proposed',
    confirmCards: { one: 'Create 1 card?', other: 'Create {count} cards?' },
    confirmDoc: { one: 'Insert 1 section?', other: 'Insert {count} sections?' },
    confirmHint: 'Written by the AI. You can undo it in one step.',
    truncated: 'The text was long: only its first part was reviewed. You can undo it in one step.',
    create: { one: 'Create card', other: 'Create {count} cards' },
    insert: { one: 'Insert section', other: 'Insert {count} sections' },
    created: { one: 'Card created', other: '{count} cards created' },
    opMany: 'Create {count} cards from clipboard',
    empty: 'The clipboard has no text.',
    tooLong: 'The clipboard text is too long (max 256 KB).',
    noBoard: 'Open a board first.',
    noLane: 'Add a lane first.',
    readOnly: 'This board is read-only.',
  },
  commands: {
    card: { newFromClipboard: 'Create card from clipboard' },
  },
};
