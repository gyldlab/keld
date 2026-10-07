// Creates the sample .drawio fixture (and a decoy secret) WITHOUT the tracing preload.
import fs from 'node:fs';
import path from 'node:path';
const P3 = path.resolve(import.meta.dir, '..');
const fix = path.join(P3, 'fixtures');
fs.mkdirSync(fix, { recursive: true });
const xml = `<mxfile host="app.diagrams.net" modified="2026-10-07T00:00:00.000Z" agent="keld-harness" version="24.0.0" type="device">
  <diagram id="p3-sample" name="Page-1">
    <mxGraphModel dx="800" dy="600" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="827" pageHeight="1169" math="0" shadow="0">
      <root>
        <mxCell id="0"/>
        <mxCell id="1" parent="0"/>
        <mxCell id="2" value="Hello" style="rounded=0;whiteSpace=wrap;html=1;" vertex="1" parent="1"><mxGeometry x="40" y="40" width="120" height="60" as="geometry"/></mxCell>
        <mxCell id="3" value="World" style="rounded=0;whiteSpace=wrap;html=1;" vertex="1" parent="1"><mxGeometry x="240" y="40" width="120" height="60" as="geometry"/></mxCell>
        <mxCell id="4" style="edgeStyle=orthogonalEdgeStyle;" edge="1" parent="1" source="2" target="3"><mxGeometry relative="1" as="geometry"/></mxCell>
      </root>
    </mxGraphModel>
  </diagram>
</mxfile>
`;
fs.writeFileSync(path.join(fix, 'sample.drawio'), xml);
fs.writeFileSync(path.join(fix, 'secret.txt'), 'not a diagram; never offered through a dialog\n');
console.log('fixtures ready:', fs.readdirSync(fix).join(', '));
