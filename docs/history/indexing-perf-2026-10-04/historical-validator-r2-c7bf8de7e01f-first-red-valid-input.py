"""Reproduce admission RED using a valid independent three-cell selection."""
import hashlib,importlib.util,json
from pathlib import Path
from test_historical import make,mod,identity
B=Path(__file__).resolve().parent
text,side=make()
# Full implemented FSM first validates this same raw event stream as28 accepted/14excluded.
assert mod.collect(text,side,identity)['accepted_rows']==28
p=B/'first-red-source.py.txt';spec=importlib.util.spec_from_loader('original_bag',loader=None);old=importlib.util.module_from_spec(spec);exec(compile(p.read_bytes(),str(p),'exec'),old.__dict__)
side['validator_sha256']=hashlib.sha256(p.read_bytes()).hexdigest()
(B/'first-red-valid-input.events.log').write_text(text)
(B/'first-red-valid-input.sidecar.json').write_text(json.dumps(side,indent=2)+'\n')
actual=old.collect(text,side,identity)
assert actual['accepted_rows']==28, ('failed-cell admission',actual['accepted_rows'],'expected28')
