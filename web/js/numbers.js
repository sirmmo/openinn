// Numbers for people, as OpenIdle's page writes them (from openidle's web/openidle.js): the
// engine's numbers are strings in its exact form (`"1.5e300"`, beyond a double `"1.25p2000"`).

/** log10 of an engine number; -Infinity for 0. */
export function log10(n) {
  if (typeof n === 'number') return Math.log10(Math.abs(n));
  const s = String(n);
  const i = s.indexOf('p');
  if (i >= 0) return Math.log10(Math.abs(parseFloat(s.slice(0, i)))) + parseInt(s.slice(i + 1), 10) * Math.LOG10_2;
  return Math.log10(Math.abs(parseFloat(s)));
}

/** The number as a double (Infinity beyond 1.8e308). */
export function toNumber(n) {
  if (typeof n === 'number') return n;
  const s = String(n);
  const i = s.indexOf('p');
  if (i >= 0) return parseFloat(s.slice(0, i)) * Math.pow(2, parseInt(s.slice(i + 1), 10));
  return parseFloat(s);
}

const SUFFIX = ['', 'K', 'M', 'B', 'T', 'Qa', 'Qi'];

/** 12.5, 1.23K, 4.56M, 7.89e42. */
export function format(n, digits = 2) {
  const s = String(n);
  const neg = s.startsWith('-');
  const l = log10(n);
  if (!isFinite(l)) return '0';
  let out;
  if (l < 3) {
    const x = Math.abs(toNumber(n));
    out = x >= 100 || Number.isInteger(x) ? String(Math.floor(x)) : x.toFixed(x < 10 ? digits : 1).replace(/\.?0+$/, '');
  } else if (l < 21) {
    const k = Math.floor(l / 3);
    out = Math.pow(10, l - 3 * k).toFixed(digits) + SUFFIX[k];
  } else {
    const e = Math.floor(l);
    const m = Math.pow(10, l - e);
    if (m >= 9.995) return (neg ? '-' : '') + '1.00e' + (e + 1);
    out = m.toFixed(digits) + 'e' + e;
  }
  return (neg ? '-' : '') + out;
}

/** 42s, 3m 05s, 2h 10m. */
export function duration(s) {
  s = Math.max(0, Math.round(s));
  if (s < 60) return s + 's';
  if (s < 3600) return Math.floor(s / 60) + 'm ' + String(s % 60).padStart(2, '0') + 's';
  return Math.floor(s / 3600) + 'h ' + String(Math.floor(s / 60) % 60).padStart(2, '0') + 'm';
}
