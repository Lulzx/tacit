// Node harness: compile Uiua -> UIR, then RUN the UIR graph, all in WASM.
// Proves the language machine (compiler + interpreter) runs in a browser.
const fs = require('fs');

const wasmPath = process.argv[2] || 'target/wasm32-unknown-unknown/release/tacit_wasm.wasm';
const bytes = fs.readFileSync(wasmPath);
const src = process.argv[3] || 'A ← ↯ [4 4] 1.0\nB ← ↯ [4 4] 2.0\nD ← ↯ [4 4] 3.0\nC ← × D + B A';

(async () => {
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const { memory, uiua_compile, uiua_run, uiua_free } = instance.exports;

  const enc = new TextEncoder();
  const srcBytes = enc.encode(src);
  const base = 1024;
  new Uint8Array(memory.buffer, base, srcBytes.length).set(srcBytes);

  // ---- compile ----
  const lenPtr = 2048;
  new Uint32Array(memory.buffer, lenPtr, 1)[0] = 0;
  const uirPtr = uiua_compile(base, srcBytes.length, lenPtr);
  const buf = memory.buffer; // re-acquire (wasm may have grown memory)
  const uirLen = new Uint32Array(buf, lenPtr, 1)[0];
  if (uirPtr === 0 || uirLen === 0) { console.log('compile FAILED'); process.exit(1); }
  const uir = new Uint8Array(buf, uirPtr, uirLen);
  console.log(`compiled: ${uirLen} UIR bytes, magic "${String.fromCharCode(...uir.slice(0,4))}"`);

  // ---- run ----
  new Uint32Array(buf, lenPtr, 1)[0] = 0;
  const valPtr = uiua_run(uirPtr, uirLen, lenPtr);
  const buf2 = memory.buffer;
  const valLen = new Uint32Array(buf2, lenPtr, 1)[0];
  if (valPtr === 0 || valLen === 0) { console.log('run FAILED'); process.exit(1); }
  const val = new Uint8Array(buf2, valPtr, valLen);

  // ---- interpret result header: [dtype, rank, shape 4x u32, data] ----
  const dtype = val[0];
  const rank = val[1];
  const shape = [];
  for (let i = 0; i < 4; i++) {
    const o = 2 + i * 4;
    shape.push(val[o] | (val[o + 1] << 8) | (val[o + 2] << 16) | (val[o + 3] << 24));
  }
  // copy data into a fresh, aligned buffer (wasm memory may be unaligned)
  const data = new Uint8Array(val.subarray(2 + 16));
  const dtypeName = { 0: 'i64', 1: 'f32', 2: 'u8' }[dtype] || `?${dtype}`;

  console.log(`ran:      dtype=${dtypeName} rank=${rank} shape=[${shape.slice(0, rank)}]`);
  const elems = shape.slice(0, rank).reduce((a, b) => a * b, 1);
  if (dtype === 1) { // f32
    const f = new Float32Array(data.buffer, 0, elems);
    console.log(`result:   [${Array.from(f.slice(0, 8)).map(x => x.toFixed(1)).join(', ')}${elems > 8 ? ', …' : ''}]`);
    console.log(`checksum: C[0] = ${f[0]}  (expect 9.0 for (1+2)*3)`);
  } else if (dtype === 0) { // i64
    const i = new BigInt64Array(data.buffer, data.byteOffset, elems);
    console.log(`result:   [${Array.from(i.slice(0, 8)).join(', ')}${elems > 8 ? ', …' : ''}]`);
  }
  console.log('OK: Uiua -> UIR -> run, all inside WASM');

  uiua_free(uirPtr, uirLen);
  uiua_free(valPtr, valLen);
})();
