from pathlib import Path
import re,json,difflib
BASE=Path('/private/tmp/flistwalker-indexing-perf-20261003/rust/target/indexing-perf-study/newtag-repair-r4')
ORIGINAL=BASE.parent/'newtag-expanded'
def end_statement(s,start,comma=False,item=False):
 p=start
 # Skip unrelated derive/allow attributes following cfg.
 while s[p:].lstrip().startswith('#['):
  p=s.index(']',p)+1
 p+=len(s[p:])-len(s[p:].lstrip())
 is_block=item or s[p:].startswith('if ') or s[p:].startswith('{')
 levels=[0,0,0];angle=0;quoted=False;escape=False;i=p
 while i<len(s):
  c=s[i]
  if quoted:
   if escape:escape=False
   elif c=='\\':escape=True
   elif c=='"':quoted=False
  elif c=='"':quoted=True
  elif s.startswith('//',i):i=s.index('\n',i);continue
  elif s.startswith('/*',i):i=s.index('*/',i+2)+2;continue
  elif c in '({[':levels['({['.index(c)]+=1
  elif c in ')}]':
   levels[')}]'.index(c)]-=1
   if c=='}' and levels==[0,0,0] and is_block:
    if not s[i+1:].lstrip().startswith('else'):return i+1
  elif comma and c=='<':angle+=1
  elif comma and c=='>' and angle:angle-=1
  elif levels==[0,0,0] and not angle and (c==';' or (comma and c==',')):return i+1
  i+=1
 raise RuntimeError('cannot determine cfg erasure: '+s[start:start+120])
def erase(s):
 s=re.sub(r'#\[cfg_attr\(test,.*?\)\]\s*','',s)
 s=re.sub(r'#\[cfg\(not\(test\)\)\]\s*','',s)
 while True:
  m=re.search(r'#\[cfg\(test\)\]',s)
  if not m:break
  p=m.end();decl=s[p:]
  decl=re.sub(r'^(\s*#\[[^\n]*\])*\s*','',decl)
  item=bool(re.match(r'(?:pub(?:\([^\n]*?\))?\s+)?(?:(?:const |async )?fn|struct|enum|impl)\b',decl))
  comma=bool(re.match(r'(?:pub(?:\([^\n]*?\))?\s+)?\w+\s*(?::|,)',decl))
  end=end_statement(s,p,comma,item);s=s[:m.start()]+s[end:]
 return s
def tokens(s):
 # Keep strings intact, discard comments/whitespace.
 pattern=r'"(?:\\.|[^"\\])*"|//[^\n]*|/\*[\s\S]*?\*/|[A-Za-z0-9_]+|[^\s]'
 return [t for t in re.findall(pattern,s) if not(t.startswith('//') or t.startswith('/*'))]
def mechanical(s):
 # Whitelist only identity capture and route binding; original decisions remain.
 s=s.replace('let Some((_victim_request_id,','let Some((_,')
 s=re.sub(r'let route = ([^;]+\.route_response\(response.request_id\));\s*match route \{',r'match \1 {',s)
 s=s.replace('let current = self.request_is_current();','').replace('if !current {','if !self.request_is_current() {')
 return s
results=[]
for tag in ['v0.27.0','v0.28.0','v0.29.0','v0.30.0']:
 base=ORIGINAL/(tag+'-basic-source');out=BASE/tag/'rust/src';record={'tag':tag,'files':{},'mismatches':[]}
 for p in out.rglob('*.rs'):
  rel=p.relative_to(out)
  if '/tests/' in str(rel) or str(rel)=='app/historical_perf.rs':continue
  old=base/rel
  if not old.exists():record['mismatches'].append(str(rel));continue
  if old.read_bytes()==p.read_bytes():continue
  a=mechanical(erase(old.read_text()));b=mechanical(erase(p.read_text()))
  same=tokens(a)==tokens(b);record['files'][str(rel)]={'cfg_disabled_tokens_match_basic':same}
  if not same:
   record['mismatches'].append(str(rel));(BASE/(tag+'-'+str(rel).replace('/','_')+'-erased.diff')).write_text(''.join(difflib.unified_diff(a.splitlines(True),b.splitlines(True),fromfile='basic-cfg-disabled',tofile='expanded-cfg-disabled')))
 print(tag,'cfg disabled mismatches:',record['mismatches']);results.append(record)
(BASE/'newtag-production-equivalence-audit.json').write_text(json.dumps(results,indent=2)+'\n')
