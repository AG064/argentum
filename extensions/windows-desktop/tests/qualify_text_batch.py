"""Actual owned UIA batch controls; no provider, real task or authority store."""
import argparse
import ctypes
import json
from pathlib import Path
import sys
import time

parser=argparse.ArgumentParser()
parser.add_argument('--adapter',type=Path,required=True)
parser.add_argument('--application',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--fixture-driver',type=Path,required=True)
parser.add_argument('--modes',default='chrome,replace,focus,move,slow,ceiling')
args=parser.parse_args()
sys.path.insert(0,str(args.fixture_driver.resolve().parent))
import qualify_native_boundaries as base
base.ADAPTER=args.adapter;base.APP=args.application;base.OUT=args.output
args.output.mkdir(exist_ok=False)

def idle():
    class Input(ctypes.Structure):
        _fields_=[('size',ctypes.c_uint),('tick',ctypes.c_uint)]
    state=Input(ctypes.sizeof(Input),0);previous=None;since=time.monotonic()
    deadline=since+20
    while time.monotonic()<deadline:
        if not ctypes.windll.user32.GetLastInputInfo(ctypes.byref(state)):raise RuntimeError('Input observation unavailable')
        if previous!=state.tick:previous=state.tick;since=time.monotonic()
        elif time.monotonic()-since>=2:return
        time.sleep(.1)
    raise RuntimeError('Owner input remained active; no launch attempted')

class Fixture(base.Fixture):
    def start(self):
        self.rpc_timeout=25
        config=json.loads(self.config.read_text())
        config['maximum_actions']=1024
        self.config.write_text(json.dumps(config),encoding='utf-8')
        super().start()

report={'provider_requests':0,'real_task_access':False,'controls':{}}
for mode in args.modes.split(','):
    assert mode in ('chrome','replace','focus','move','slow','ceiling')
    fixture=Fixture(mode,('--text-batch-'+mode,))
    try:
        idle();fixture.launch()
        state,field=fixture.target(fixture.observe(),text=True)
        assert not fixture.act(state,field,'select_all')['isError']
        state,field=fixture.target(fixture.observe(),text=True)
        marker='Bounded same-target marker '*7
        started=time.monotonic()
        result=fixture.act(state,field,'replace_selection',value=marker)
        elapsed=time.monotonic()-started
        if mode in ('chrome','slow'):
            assert not result['isError'],result
            state,field=fixture.target(fixture.observe(),text=True)
            assert field['text']==marker
            if mode=='slow':assert 5<elapsed<20,elapsed
        elif mode=='ceiling':
            assert result['isError'],result
            reason=result['structuredContent']['reason']
            # A scheduled native message can hit the stricter250ms bound first.
            # Do not treat that honest uncertain effect as a successful batch.
            assert reason in ('provider_timeout_effect_uncertain_connection_retired','input_timeout_effect_uncertain'),result
            assert elapsed<24.5,elapsed
            if reason=='provider_timeout_effect_uncertain_connection_retired':assert elapsed>=19,elapsed
        else:
            assert result['isError'] and result['structuredContent']['reason']=='partial_text_effect_uncertain',result
            if mode=='replace':
                state,field=fixture.target(fixture.observe(),text=True)
                # The application's callback can move the first admitted
                # character into its new control inside that same message.
                # No subsequent character may be sent to the replacement.
                assert field['text'] in ('Untouched replacement','Untouched replacement'+marker[0]),repr(field['text'])
        report['controls'][mode]={'passed':True,'is_error':result['isError'],'action_seconds':elapsed}
    finally:
        teardown=fixture.close()
        report['controls'].setdefault(mode,{})['teardown']=teardown
        if mode not in ('chrome','slow'):assert teardown['api_returns']<teardown['reserved_actions']
        (args.output/'report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
print(json.dumps(report))
