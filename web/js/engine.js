// The engine as WebAssembly: JSON requests in, JSON answers out (see crates/openinn-web).

export class Engine {
  static async load(url = 'openinn.wasm') {
    const res = await fetch(url);
    const { instance } = await WebAssembly.instantiate(await res.arrayBuffer(), {});
    return new Engine(instance.exports);
  }

  constructor(wasm) {
    this.wasm = wasm;
  }

  call(request) {
    const w = this.wasm;
    const bytes = new TextEncoder().encode(JSON.stringify(request));
    const ptr = w.inn_alloc(bytes.length);
    new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes);
    const out = w.inn_call(ptr, bytes.length);
    return JSON.parse(new TextDecoder().decode(new Uint8Array(w.memory.buffer, out, w.inn_out_len())));
  }

  start(files, seed, commands = null) {
    return this.call({ op: 'start', files, seed, commands });
  }

  apply(command) {
    return this.call({ op: 'apply', command });
  }

  save() {
    return this.call({ op: 'save' });
  }
}

// A ruleset's files, listed in games/<id>/files.json by tools/site.sh.
export async function fetchRuleset(id) {
  const base = `games/${id}/`;
  const names = await (await fetch(base + 'files.json')).json();
  return Promise.all(names.map(async (n) => [n, await (await fetch(base + n)).text()]));
}
