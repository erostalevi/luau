// Natural-language dates (en / es / pt) → ISO `YYYY-MM-DD`.
//   today · tomorrow · yesterday · next week · monday · next friday · in 3 days
//   oct 3 · 3 oct · 3/10 · hoy · mañana · pasado mañana · lunes · hoje · amanhã

const WEEKDAYS: Record<string, number> = {
  sunday: 0, sun: 0, domingo: 0, dom: 0,
  monday: 1, mon: 1, lunes: 1, lun: 1, segunda: 1, 'segunda-feira': 1, seg: 1,
  tuesday: 2, tue: 2, tues: 2, martes: 2, mar: 2, terça: 2, terca: 2, 'terça-feira': 2, ter: 2,
  wednesday: 3, wed: 3, miércoles: 3, miercoles: 3, mié: 3, mie: 3, quarta: 3, 'quarta-feira': 3, qua: 3,
  thursday: 4, thu: 4, thurs: 4, jueves: 4, jue: 4, quinta: 4, 'quinta-feira': 4, qui: 4,
  friday: 5, fri: 5, viernes: 5, vie: 5, sexta: 5, 'sexta-feira': 5, sex: 5,
  saturday: 6, sat: 6, sábado: 6, sabado: 6, sáb: 6, sab: 6,
};

const MONTHS: Record<string, number> = {
  jan: 0, january: 0, ene: 0, enero: 0, janeiro: 0,
  feb: 1, february: 1, febrero: 1, fev: 1, fevereiro: 1,
  mar: 2, march: 2, marzo: 2, março: 2, marco: 2,
  apr: 3, april: 3, abr: 3, abril: 3,
  may: 4, mayo: 4, mai: 4, maio: 4,
  jun: 5, june: 5, junio: 5, junho: 5,
  jul: 6, july: 6, julio: 6, julho: 6,
  aug: 7, august: 7, ago: 7, agosto: 7,
  sep: 8, sept: 8, september: 8, septiembre: 8, set: 8, setembro: 8,
  oct: 9, october: 9, octubre: 9, out: 9, outubro: 9,
  nov: 10, november: 10, noviembre: 10, novembro: 10,
  dec: 11, december: 11, dic: 11, diciembre: 11, dez: 11, dezembro: 11,
};

const pad = (n: number) => String(n).padStart(2, '0');
export const iso = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

function addDays(base: Date, n: number): Date {
  const d = new Date(base);
  d.setDate(d.getDate() + n);
  return d;
}

export function parseNaturalDate(input: string, now = new Date()): string | null {
  const s = input.trim().toLowerCase().replace(/\s+/g, ' ');
  if (!s) return null;
  if (/^\d{4}-\d{2}-\d{2}$/.test(s)) return s;
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const simple: Record<string, number> = {
    today: 0, now: 0, hoy: 0, hoje: 0,
    tomorrow: 1, tmr: 1, mañana: 1, manana: 1, amanhã: 1, amanha: 1,
    yesterday: -1, ayer: -1, ontem: -1,
    'pasado mañana': 2, 'pasado manana': 2, 'depois de amanhã': 2, 'depois de amanha': 2,
    'day after tomorrow': 2,
  };
  if (s in simple) return iso(addDays(today, simple[s]));
  if (/^(next week|la semana que viene|la próxima semana|la proxima semana|semana que vem|próxima semana|proxima semana)$/.test(s)) {
    return iso(addDays(today, ((8 - today.getDay()) % 7) || 7));
  }
  let m = /^(?:in|en|em|dentro de) (\d+) (days?|días?|dias?|weeks?|semanas?)$/.exec(s);
  if (m) {
    const n = Number(m[1]) * (/^(week|semana)/.test(m[2]) ? 7 : 1);
    return iso(addDays(today, n));
  }
  m = /^(?:next |el próximo |el proximo |próximo |proximo |próxima |proxima |this |este |esta )?([a-záéíóúçãõ-]+)$/.exec(s);
  if (m && m[1] in WEEKDAYS) {
    const target = WEEKDAYS[m[1]];
    let diff = (target - today.getDay() + 7) % 7;
    if (diff === 0 || /^(next|el próximo|el proximo|próximo|proximo|próxima|proxima)/.test(s)) diff = diff || 7;
    return iso(addDays(today, diff));
  }
  // "oct 3", "october 3 2027", "3 oct", "3 de octubre", "3 de outubro de 2027"
  m = /^([a-záéíóúçãõ]+)\.? (\d{1,2})(?:,? (\d{4}))?$/.exec(s) ?? null;
  let day: number | null = null;
  let month: number | null = null;
  let year: number | null = null;
  if (m && m[1] in MONTHS) {
    month = MONTHS[m[1]];
    day = Number(m[2]);
    year = m[3] ? Number(m[3]) : null;
  } else {
    const m2 = /^(\d{1,2})(?: de)? ([a-záéíóúçãõ]+)\.?(?:(?: de)? (\d{4}))?$/.exec(s);
    if (m2 && m2[2] in MONTHS) {
      day = Number(m2[1]);
      month = MONTHS[m2[2]];
      year = m2[3] ? Number(m2[3]) : null;
    }
  }
  if (day === null) {
    // d/m (es/pt style) or m/d when navigator locale is en-US
    const m3 = /^(\d{1,2})[/.](\d{1,2})(?:[/.](\d{2,4}))?$/.exec(s);
    if (m3) {
      const us = typeof navigator !== 'undefined' && /^en-US/i.test(navigator.language);
      day = Number(us ? m3[2] : m3[1]);
      month = Number(us ? m3[1] : m3[2]) - 1;
      year = m3[3] ? Number(m3[3].length === 2 ? `20${m3[3]}` : m3[3]) : null;
    }
  }
  if (day !== null && month !== null && day >= 1 && day <= 31 && month >= 0 && month <= 11) {
    let y = year ?? today.getFullYear();
    let d = new Date(y, month, day);
    if (year === null && d < addDays(today, -30)) d = new Date(++y, month, day);
    if (d.getMonth() !== month) return null;
    return iso(d);
  }
  return null;
}
