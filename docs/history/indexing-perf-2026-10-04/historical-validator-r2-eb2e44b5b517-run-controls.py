"""Record actual Python control results; no Rust/performance execution."""
from pathlib import Path
import datetime,hashlib,json,sys,time,unittest
B=Path(__file__).resolve().parent
before={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in B.glob('*.py')}
records=[]
class Recorded(unittest.TextTestResult):
    def addSuccess(self,test):super().addSuccess(test);records.append({'test':test.id(),'outcome':'PASS'})
    def addFailure(self,test,err):super().addFailure(test,err);records.append({'test':test.id(),'outcome':'FAIL'})
    def addError(self,test,err):super().addError(test,err);records.append({'test':test.id(),'outcome':'ERROR'})
    def addSkip(self,test,reason):super().addSkip(test,reason);records.append({'test':test.id(),'outcome':'SKIP','reason':reason})
log=B/'final-controls.log';side=B/'final-controls.json';assert not log.exists() and not side.exists()
start=time.monotonic();suite=unittest.defaultTestLoader.discover(str(B),'test_historical.py')
with log.open('w') as f:r=unittest.TextTestRunner(stream=f,verbosity=2,resultclass=Recorded).run(suite)
code=0 if r.wasSuccessful() else 1
x={'evidence_kind':'synthetic-control','scope':'Historical validator controls only; no historical benchmark or libtest run','exit_code':code,'actual_test_count':r.testsRun,'passed':sum(v['outcome']=='PASS' for v in records),'failed':len(r.failures),'errors':len(r.errors),'skipped':len(r.skipped),'records':records,'elapsed_seconds':time.monotonic()-start,'source_before':before,'source_after':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in B.glob('*.py')},'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest(),'finished_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()};side.write_text(json.dumps(x,indent=2)+'\n');print('actual controls',r.testsRun,'PASS',x['passed'],'FAIL',x['failed'],'ERROR',x['errors'],'SKIP',x['skipped']);raise SystemExit(code)
