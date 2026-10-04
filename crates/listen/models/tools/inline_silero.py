import onnx, numpy as np, onnxruntime as ort, copy
from onnx import helper, TensorProto
m = onnx.load('silero_vad_16k_op15.onnx')
feeds = {'input': np.zeros((1,576),np.float32), 'state': np.zeros((2,1,128),np.float32), 'sr': np.array(16000,np.int64)}
def cond_values(m, names):
    mm = copy.deepcopy(m)
    del mm.graph.output[:]
    for n in names: mm.graph.output.append(helper.make_tensor_value_info(n, TensorProto.BOOL, None))
    s = ort.InferenceSession(mm.SerializeToString(), providers=['CPUExecutionProvider'])
    return dict(zip(names, [bool(np.asarray(v).reshape(-1)[0]) for v in s.run(None, feeds)]))
for rnd in range(10):
    ifs = [n for n in m.graph.node if n.op_type == 'If']
    if not ifs: break
    vals = cond_values(m, [n.input[0] for n in ifs])
    new = []
    for n in m.graph.node:
        if n.op_type != 'If': new.append(n); continue
        br = {a.name: a.g for a in n.attribute}['then_branch' if vals[n.input[0]] else 'else_branch']
        for x in br.node: new.append(x)
        for o_in, o_out in zip(br.output, n.output):
            new.append(helper.make_node('Identity', [o_in.name], [o_out]))
        for init in br.initializer: m.graph.initializer.append(init)
    del m.graph.node[:]; m.graph.node.extend(new)
    print('round', rnd, 'inlined', len(ifs))
# sr input no longer needed? keep it, tract fixes it as constant
onnx.checker.check_model(m)
onnx.save(m, 'silero_vad_inlined.onnx')
# verify equivalence on random audio
s0 = ort.InferenceSession('silero_vad_16k_op15.onnx', providers=['CPUExecutionProvider'])
s1 = ort.InferenceSession('silero_vad_inlined.onnx', providers=['CPUExecutionProvider'])
rng=np.random.default_rng(0); st=np.zeros((2,1,128),np.float32); st1=st.copy()
for i in range(20):
    x=(rng.standard_normal((1,576))*0.1).astype(np.float32)
    a=s0.run(None, {'input':x,'state':st,'sr':np.array(16000,np.int64)}); b=s1.run(None, {'input':x,'state':st1,'sr':np.array(16000,np.int64)})
    assert np.allclose(a[0],b[0],atol=1e-5) and np.allclose(a[1],b[1],atol=1e-5); st,st1=a[1],b[1]
print('equivalent', sorted(set(n.op_type for n in m.graph.node)))
