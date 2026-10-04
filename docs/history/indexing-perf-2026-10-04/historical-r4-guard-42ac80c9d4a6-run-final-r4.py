from pathlib import Path
import subprocess,sys,json
b=Path(__file__).resolve().parent
for tag in ['v0.28.0','v0.29.0','v0.30.0']:
 short=tag.replace('0.','').replace('.0','')
 root=b.parent/'newtag-repair-r4'/tag/'rust';target=b/('target-r4-'+tag)
 for suffix,mode in [('historical-discovery','list-pattern'),('historical-guards','pattern')]:
  stage='r4-'+short+'-'+suffix
  subprocess.run([sys.executable,str(b/'run-stage-bounded-r3.py'),stage,str(root),str(target),'historical_',mode],check=True)
  d=json.loads((b/(stage+'.json')).read_text());assert d['exit_code']==0 and d['continuation_authorized'],d
  if mode=='list-pattern':assert len(d['discovered_tests'])==16,d
 subprocess.run([sys.executable,str(b/'run-neighbors-bounded-r4.py'),tag],check=True)
