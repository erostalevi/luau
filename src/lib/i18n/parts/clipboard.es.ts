// Crear tarjeta desde el portapapeles (tal cual / revisión con IA local).
export default {
  clipboard: {
    how: '¿Cómo convertir el portapapeles en tarjetas?',
    asIs: 'Tal cual',
    asIsDesc: 'Una tarjeta: la primera línea es el título y el resto queda igual',
    aiReview: 'Que la IA lo revise',
    aiReviewDesc: 'La IA local lo divide en tarjetas con títulos, tareas, etiquetas y propiedades',
    reviewing: 'La IA local está leyendo el portapapeles…',
    aiFailed: 'La IA local no pudo crear tarjetas: {message}',
    noCards: 'no propuso ninguna tarjeta',
    confirmCards: { one: '¿Crear 1 tarjeta?', other: '¿Crear {count} tarjetas?' },
    confirmDoc: { one: '¿Insertar 1 sección?', other: '¿Insertar {count} secciones?' },
    confirmHint: 'Escrito por la IA local en este computador. Puedes deshacerlo en un paso.',
    truncated: 'El texto era largo: solo se revisó la primera parte. Puedes deshacerlo en un paso.',
    create: { one: 'Crear tarjeta', other: 'Crear {count} tarjetas' },
    insert: { one: 'Insertar sección', other: 'Insertar {count} secciones' },
    created: { one: 'Tarjeta creada', other: '{count} tarjetas creadas' },
    opMany: 'Crear {count} tarjetas desde el portapapeles',
    empty: 'El portapapeles no tiene texto.',
    tooLong: 'El texto del portapapeles es demasiado largo (máx. 256 KB).',
    noBoard: 'Abre un tablero primero.',
    noLane: 'Agrega una lista primero.',
    readOnly: 'Este tablero es de solo lectura.',
  },
  commands: {
    card: { newFromClipboard: 'Crear tarjeta desde el portapapeles' },
  },
};
