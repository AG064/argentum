"""Real owned Win32 menus and UIA controls. No model, credentials or real task."""
import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import sys
import time

parser = argparse.ArgumentParser()
parser.add_argument('--adapter', type=Path, required=True)
parser.add_argument('--application', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--fixture-driver', type=Path, required=True)
parser.add_argument('--diagnostic-wrapper', type=Path)
args = parser.parse_args()
# Reuse the existing bounded fixture and current-reference protocol driver.
sys.path.insert(0, str(args.fixture_driver.resolve().parent))
import qualify_native_boundaries as base
base.ADAPTER = args.adapter
base.APP = args.application
base.OUT = args.output
if args.diagnostic_wrapper:
    original_popen = base.subprocess.Popen
    def diagnostic_popen(argv, *positional, **keywords):
        if isinstance(argv,list) and argv[0]==str(args.adapter):
            argv=[str(args.diagnostic_wrapper),*argv]
        return original_popen(argv,*positional,**keywords)
    base.subprocess.Popen=diagnostic_popen
args.output.mkdir(exist_ok=False)

class Fixture(base.Fixture):
    freshness_rejections = 0
    unavailable_inspections = 0
    def start(self):
        cfg = json.loads(self.config.read_text())
        cfg.update(targeted_menu_input=True, menu_paths=[['File','Open...'],['File','Save As...'],['Edit','Select All'],['Search','Find...'],['Search','Find Next']], lifetime_seconds=90)
        cfg['invokable_names'] += ['File','Open...','Open','Save As...','Edit','Select All','Search','Find...','Find Next','Close']
        cfg['invokable_names'] = list(dict.fromkeys(cfg['invokable_names']))
        cfg['dialogs'] += ['Open','Find']
        self.config.write_text(json.dumps(cfg), encoding='utf-8')
        super().start()
    def observe(self, offset=0):
        for attempt in range(5):
            value = self.call('desktop_observe',text_limit=256,offset=offset,element_limit=32)
            if not value['isError']:
                return value['structuredContent']
            detail = value['structuredContent']
            # Only fresh read-only inspection is repeated, never an input effect.
            transient = detail['reason']=='foreground_unavailable_reobserve' or (detail['reason']=='native_provider_failure_reobserve_no_effect_claim' and detail.get('provider_failure_class')=='element_unavailable')
            if not transient:
                raise AssertionError(value)
            self.unavailable_inspections += 1
            time.sleep(.2)
        raise AssertionError(value)
    def locate(self, predicate):
        state = self.observe()
        while True:
            for element in state['elements']:
                if predicate(element):
                    return state, element
            assert state['next_offset'] is not None, 'Current scoped control missing'
            state = self.observe(state['next_offset'])
    def menu(self, name):
        return self.locate(lambda e: e['role']=='Win32.MenuCommand' and e['name']==name)
    def invoke_menu(self, name):
        value = self.current_action(lambda e: e['role']=='Win32.MenuCommand' and e['name']==name, 'invoke')
        assert not value['isError'], value
        assert value['structuredContent']['observed_outcome_verified'] is False
        return value
    def filename(self):
        return self.locate(lambda e: e['role']=='ControlType.Edit' and 'ValuePatternIdentifiers.Pattern' in e['patterns'] and e['name'].startswith('File name'))
    def current_action(self, predicate, action, **values):
        # Dialog population can invalidate an observation. Never resend the old
        # handle or repeat an uncertain effect. Only pre-admission stale denials
        # with unchanged primitive counts permit fresh observation/resubmission.
        for attempt in range(5):
            state, target = self.locate(predicate)
            result = self.act(state, target, action, **values)
            if not result['isError']:
                return result
            detail = result['structuredContent']
            if not detail['reason'].startswith('stale_'):
                return result
            assert detail['actions']==state['actions'], 'Stale rejection had an admitted primitive; do not retry'
            self.freshness_rejections += 1
            time.sleep(.2)
        return result

def input_tick():
    class Last(ctypes.Structure):
        _fields_ = [('size',ctypes.c_uint),('tick',ctypes.c_uint)]
    value = Last(ctypes.sizeof(Last),0)
    assert ctypes.windll.user32.GetLastInputInfo(ctypes.byref(value))
    return value.tick

f = Fixture('native-menu', ('--native-menu-controls',))
report = dict(provider_requests=0, real_task_access=False, controls=[])
try:
    f.launch()
    original_root = base.own_window(f.app_pid)
    state, select = f.menu('Select All')
    base.denied(f.act(state,select,'set_value',value='not a menu action'),'invoke_only')
    tick = input_tick()
    result = f.act(state,select,'invoke')
    assert not result['isError'],result
    assert result['structuredContent']['native_menu_action']=='select_all'
    time.sleep(.2)
    assert input_tick()==tick, 'Native menu invocation synthesized global input or owner intervened'
    report['controls'].append('current_native_selection_without_global_input')
    base.denied(f.act(state,select,'invoke'),'stale')
    report['controls'].append('stale_menu_reference_denied')
    state, doc = f.locate(lambda e: 'TextPatternIdentifiers.Pattern' in e['patterns'])
    assert doc['targeted_text_input_enabled'] and 'TargetedTextInput' in doc['patterns']
    assert doc['targeted_text_max_utf16']==256
    text = 'Native menu control\nUnicode: café 日本語\n'
    result = f.act(state,doc,'replace_selection',value=text)
    assert not result['isError'],result
    state, doc = f.locate(lambda e: 'TextPatternIdentifiers.Pattern' in e['patterns'])
    assert doc['text'].replace('\r\n','\n')==text
    report['controls'].append('selection_then_exact_unicode_readback')
    f.invoke_menu('Save As...');time.sleep(1)
    filename=lambda e: e['role']=='ControlType.Edit' and 'ValuePatternIdentifiers.Pattern' in e['patterns'] and e['name'].startswith('File name')
    base.denied(f.current_action(filename,'set_value',value=str(f.root/'outside.txt')),'destination_denied')
    result=f.current_action(filename,'set_value',value=str(f.root/'output.txt'))
    assert not result['isError'],result
    result=f.current_action(lambda e: e['name']=='Save' and 'InvokePatternIdentifiers.Pattern' in e['patterns'],'invoke')
    assert not result['isError'],result
    deadline=time.monotonic()+3
    while not (f.root/'output.txt').exists() and time.monotonic()<deadline:
        time.sleep(.1)
    assert (f.root/'output.txt').read_text(encoding='utf-8-sig').replace('\r\n','\n')==text
    assert (f.root/'input.txt').read_bytes()==f.before
    report['controls'].append('native_menu_save_as_actual_file_and_source_preservation')
    f.invoke_menu('Open...');time.sleep(1)
    result=f.current_action(filename,'set_value',value=str(f.root/'output.txt'))
    assert not result['isError'],result
    result=f.current_action(lambda e: e['name']=='Open' and 'InvokePatternIdentifiers.Pattern' in e['patterns'],'invoke')
    assert not result['isError'],result
    time.sleep(.2)
    state, doc = f.locate(lambda e: 'TextPatternIdentifiers.Pattern' in e['patterns'])
    assert doc['text'].replace('\r\n','\n')==text
    report['controls'].append('native_menu_open_and_current_reopened_readback')
    f.invoke_menu('Find...');time.sleep(1)
    state, field = f.locate(lambda e: e['role']=='ControlType.Edit' and 'ValuePatternIdentifiers.Pattern' in e['patterns'])
    assert not f.act(state,field,'set_value',value='Unicode')['isError']
    state, button = f.locate(lambda e: e['name']=='Find Next' and 'InvokePatternIdentifiers.Pattern' in e['patterns'])
    assert not f.act(state,button,'invoke')['isError']
    state, button = f.locate(lambda e: e['name']=='Close' and 'InvokePatternIdentifiers.Pattern' in e['patterns'])
    assert not f.act(state,button,'invoke')['isError']
    report['controls'].append('native_menu_find_and_owned_dialog_controls')
    state = f.observe()
    image = f.ok('desktop_screenshot',generation=state['generation'])
    assert hashlib.sha256((f.root/'images'/image['artifact']).read_bytes()).hexdigest()==image['sha256']
    report['controls'].append('current_window_image_after_reopen_and_find')
    report['state']='passed'
except Exception as error:
    report.update(state='failed',error_type=type(error).__name__,detail=str(error)[:1024])
    user=ctypes.windll.user32
    user.GetForegroundWindow.restype=ctypes.c_void_p
    user.GetWindowThreadProcessId.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_uint)]
    foreground=user.GetForegroundWindow();pid=ctypes.c_uint()
    user.GetWindowThreadProcessId(foreground,ctypes.byref(pid))
    report['foreground_is_owned_process']=pid.value==f.app_pid
    user.GetAncestor.argtypes=[ctypes.c_void_p,ctypes.c_uint]
    user.GetAncestor.restype=ctypes.c_void_p
    user.GetWindow.argtypes=[ctypes.c_void_p,ctypes.c_uint]
    user.GetWindow.restype=ctypes.c_void_p
    report['foreground_is_original_root']=foreground==original_root
    report['foreground_root_owner_is_original_root']=user.GetAncestor(foreground,3)==original_root
    report['foreground_has_direct_owner']=bool(user.GetWindow(foreground,4))
    user.IsWindow.argtypes=[ctypes.c_void_p]
    report['original_root_still_exists']=bool(user.IsWindow(original_root))
    user.GetClassNameW.argtypes=[ctypes.c_void_p,ctypes.c_wchar_p,ctypes.c_int]
    chain=[];current=foreground
    for depth in range(8):
        if not current: break
        owner_pid=ctypes.c_uint();user.GetWindowThreadProcessId(current,ctypes.byref(owner_pid))
        if owner_pid.value!=f.app_pid:
            chain.append({'owned_process':False});break
        label=ctypes.create_unicode_buffer(128);user.GetClassNameW(current,label,128)
        chain.append({'owned_process':True,'original_root':current==original_root,'class':label.value})
        current=user.GetWindow(current,4)
    report['scoped_owner_chain']=chain
    raise
finally:
    report['teardown']=f.close()
    report['producer_exception_metadata']=f.p.stderr.read()[:4096]
    report['pre_admission_stale_rejections_with_fresh_observation']=f.freshness_rejections
    report['fresh_read_only_inspections_after_unavailable']=f.unavailable_inspections
    (args.output/'report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    print(json.dumps(report))
