// Node test of the graph-rendering logic against the real uiua_graph output.
// Mirrors the parseGraph/renderGraph functions from index.html.
const fs = require('fs');
const wasmPath = 'target/wasm32-unknown-unknown/release/tacit_wasm.wasm';
const src = process.argv[2] || 'A ← ↯ [4 4] 1.0\nB ← ↯ [4 4] 2.0\nD ← ↯ [4 4] 3.0\nC ← × D + B A';

const OP = {0:'Const',1:'Add',2:'Sub',3:'Multiply',4:'Div',5:'Neg',6:'ReduceSum',7:'Reshape',8:'Display',9:'Keyboard',10:'GraphNodes',11:'GraphEdges',12:'MachineDesc',13:'ReadySet',14:'Reverse',15:'Count',16:'Format',17:'Fill',18:'Copy',19:'Send',20:'BytesMoved',21:'BytesCopied',22:'KernelEntries',23:'AddMul',24:'Rows',25:'Caps',26:'Names',27:'Zero',28:'FmtMachine',29:'Provenance',30:'Stats',31:'GradeUp',32:'GradeDown',33:'Select',34:'Keep',35:'Pick',36:'Equal',37:'Hash',38:'Store',39:'Load',40:'Clock',41:'ReplayKeys',42:'ReplayClock',43:'Trace',44:'Matmul',45:'Couple',46:'Request'};

(async () => {
  const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
  const { memory, uiua_compile, uiua_graph, uiua_free } = instance.exports;
  const enc = new TextEncoder();
  const srcBytes = enc.encode(src);
  const base = 1024;
  new Uint8Array(memory.buffer, base, srcBytes.length).set(srcBytes);
  const lenPtr = 2048;
  new Uint32Array(memory.buffer, lenPtr, 1)[0] = 0;
  const uirPtr = uiua_compile(base, srcBytes.length, lenPtr);
  const buf = memory.buffer;
  const uirLen = new Uint32Array(buf, lenPtr, 1)[0];
  new Uint32Array(buf, lenPtr, 1)[0] = 0;
  const gPtr = uiua_graph(uirPtr, uirLen, lenPtr);
  const buf2 = memory.buffer;
  const gLen = new Uint32Array(buf2, lenPtr, 1)[0];
  const blob = new Uint8Array(buf2.slice(gPtr, gPtr + gLen));

  // parseGraph
  const dv = new DataView(blob.buffer, blob.byteOffset);
  const count = dv.getUint32(0, true);
  const version = dv.getUint32(4, true);
  const nodes = [];
  let o = 8;
  for (let i = 0; i < count; i++) {
    const id = dv.getUint32(o, true); o += 4;
    const op = blob[o]; o += 1;
    const dtype = blob[o]; o += 1;
    const rank = blob[o]; o += 1;
    const engine = blob[o]; o += 1;
    const pure = blob[o]; o += 1;
    const shape = [];
    for (let k = 0; k < 4; k++) { shape.push(dv.getUint32(o, true)); o += 4; }
    const nameLen = dv.getUint32(o, true); o += 4;
    const name = new TextDecoder().decode(blob.subarray(o, o + nameLen)); o += nameLen;
    const in0 = dv.getUint32(o, true); o += 4;
    const in1 = dv.getUint32(o, true); o += 4;
    const in2 = dv.getUint32(o, true); o += 4;
    nodes.push({ id, op, dtype, rank, engine, pure, shape, name, in0, in1, in2 });
  }
  console.log(`graph v${version}`);
  console.log(`graph: ${count} nodes`);
  for (const n of nodes) {
    const ins = [n.in0, n.in1, n.in2].filter(x => x !== 0xffffffff).join(',');
    console.log(`  id=${n.id} op=${OP[n.op]||n.op} name="${n.name}" in=[${ins}]`);
  }
  uiua_free(uirPtr, uirLen);
  uiua_free(gPtr, gLen);
})();
