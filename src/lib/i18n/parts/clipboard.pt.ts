// Criar cartão a partir da área de transferência (como está / revisão com IA).
export default {
  clipboard: {
    how: 'Como transformar a área de transferência em cartões?',
    asIs: 'Como está',
    asIsDesc: 'Um cartão: a primeira linha é o título e o resto fica igual',
    aiReview: 'Deixar a IA revisar',
    aiReviewDesc: 'A IA divide em cartões com títulos, tarefas, tags e propriedades',
    reviewing: 'A IA está lendo o texto…',
    aiFailed: 'A IA não conseguiu criar cartões: {message}',
    noCards: 'nenhum cartão foi proposto',
    confirmCards: { one: 'Criar 1 cartão?', other: 'Criar {count} cartões?' },
    confirmDoc: { one: 'Inserir 1 seção?', other: 'Inserir {count} seções?' },
    confirmHint: 'Escrito pela IA. Você pode desfazer em um passo.',
    truncated: 'O texto era longo: só a primeira parte foi revisada. Você pode desfazer em um passo.',
    create: { one: 'Criar cartão', other: 'Criar {count} cartões' },
    insert: { one: 'Inserir seção', other: 'Inserir {count} seções' },
    created: { one: 'Cartão criado', other: '{count} cartões criados' },
    opMany: 'Criar {count} cartões a partir da área de transferência',
    empty: 'A área de transferência não tem texto.',
    tooLong: 'O texto da área de transferência é longo demais (máx. 256 KB).',
    noBoard: 'Abra um quadro primeiro.',
    noLane: 'Adicione uma lista primeiro.',
    readOnly: 'Este quadro é somente leitura.',
  },
  commands: {
    card: { newFromClipboard: 'Criar cartão a partir da área de transferência' },
  },
};
