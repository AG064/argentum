// Optional current-user Windows adapter. UIA operations stay on one bounded MTA
// worker; a timed-out effect is uncertain and retires the whole connection.
// No network, shell, desktop-wide text/capture, clipboard or arbitrary keys.
using System;
using System.Collections.Generic;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Web.Script.Serialization;
using System.Windows.Automation;
using System.Windows.Forms;

namespace Argentum.WindowsDesktop
{
    internal sealed class Denied : Exception { public Denied(string reason) : base(reason) {} }

    internal static class Native
    {
        [StructLayout(LayoutKind.Sequential)] internal struct Rect { public int Left, Top, Right, Bottom; }
        [StructLayout(LayoutKind.Sequential)] internal struct LastInput { public uint Size, Time; }
        [DllImport("user32.dll")] internal static extern bool IsWindow(IntPtr window);
        [DllImport("user32.dll")] internal static extern bool IsWindowVisible(IntPtr window);
        [DllImport("user32.dll")] internal static extern bool IsWindowEnabled(IntPtr window);
        [DllImport("user32.dll")] internal static extern bool IsIconic(IntPtr window);
        [DllImport("user32.dll")] internal static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")] internal static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll")] internal static extern bool SetProcessDpiAwarenessContext(IntPtr context);
        [DllImport("user32.dll")] internal static extern uint GetDpiForWindow(IntPtr window);
        [DllImport("user32.dll")] internal static extern IntPtr GetWindowDpiAwarenessContext(IntPtr window);
        [DllImport("user32.dll")] internal static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
        [DllImport("user32.dll")] internal static extern IntPtr GetAncestor(IntPtr window, uint flags);
        [DllImport("user32.dll")] internal static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
        [DllImport("user32.dll")] internal static extern bool GetWindowRect(IntPtr window, out Rect rect);
        [DllImport("user32.dll")] internal static extern bool GetWindowDisplayAffinity(IntPtr window, out uint affinity);
        [DllImport("user32.dll")] internal static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
        [DllImport("user32.dll")] internal static extern bool GetLastInputInfo(ref LastInput input);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] internal static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr wParam, IntPtr lParam, uint flags, uint timeout, out UIntPtr result);
        [DllImport("dwmapi.dll")] internal static extern int DwmGetWindowAttribute(IntPtr window, uint attribute, out int value, int size);
        [DllImport("user32.dll")] internal static extern IntPtr GetThreadDesktop(uint thread);
        [DllImport("kernel32.dll")] internal static extern uint GetCurrentThreadId();
        [DllImport("user32.dll", CharSet=CharSet.Unicode)] internal static extern bool GetUserObjectInformation(IntPtr handle, int index, StringBuilder data, uint length, out uint needed);
        [DllImport("user32.dll")] internal static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
        [DllImport("user32.dll")] internal static extern bool CloseDesktop(IntPtr desktop);
        [DllImport("advapi32.dll")] private static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
        [DllImport("advapi32.dll")] private static extern bool GetTokenInformation(IntPtr token,int kind,out int value,uint size,out uint needed);
        [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr handle);
        internal static bool OrdinaryToken(IntPtr process)
        {
            IntPtr token; if(!OpenProcessToken(process,8,out token)) return false;
            try { int elevation,uiAccess;uint needed;return GetTokenInformation(token,20,out elevation,4,out needed) && elevation==0 && GetTokenInformation(token,26,out uiAccess,4,out needed) && uiAccess==0; }
            finally { CloseHandle(token); }
        }
        internal static uint InputTime() { var value = new LastInput { Size = (uint)Marshal.SizeOf(typeof(LastInput)) }; if (!GetLastInputInfo(ref value)) throw new Denied("input_state_unavailable"); return value.Time; }
        internal static string DesktopName(IntPtr handle) { var b = new StringBuilder(256); uint needed; if (handle == IntPtr.Zero || !GetUserObjectInformation(handle, 2, b, 512, out needed)) throw new Denied("desktop_identity_unavailable"); return b.ToString(); }
        internal static string InteractiveDesktop() { var d = OpenInputDesktop(0, false, 1); try { return DesktopName(d); } finally { if (d != IntPtr.Zero) CloseDesktop(d); } }
    }

    internal sealed class Config
    {
        public string Program, Sha256, WorkingDirectory, ArtifactDirectory, ActionLedger, CampaignId;
        public string[] Arguments, Documents, SaveDestinations, Dialogs, Actions, InvokableNames, TextControlClasses;
        public bool TargetedTextInput;
        public int MaximumActions, LifetimeSeconds;
        internal static Config Load(string file)
        {
            var bytes = File.ReadAllBytes(file);
            if (bytes.Length > 32768) throw new Denied("configuration_capacity");
            var map = Json.Read(Encoding.UTF8.GetString(bytes));
            Json.Keys(map, "program", "sha256", "working_directory", "artifact_directory", "action_ledger", "campaign_id", "arguments", "documents", "save_destinations", "dialogs", "actions", "invokable_names", "text_control_classes", "targeted_text_input", "maximum_actions", "lifetime_seconds");
            var c = new Config { Program=Json.Text(map,"program"), Sha256=Json.Text(map,"sha256"), WorkingDirectory=Json.Text(map,"working_directory"), ArtifactDirectory=Json.Text(map,"artifact_directory"), Arguments=Json.Strings(map,"arguments"), Documents=Json.Strings(map,"documents"), Dialogs=Json.Strings(map,"dialogs"), Actions=Json.Strings(map,"actions"), InvokableNames=Json.Strings(map,"invokable_names"), TargetedTextInput=Json.Boolean(map,"targeted_text_input"), MaximumActions=Json.Number(map,"maximum_actions"), LifetimeSeconds=Json.Number(map,"lifetime_seconds") };
            if (!Path.IsPathRooted(c.Program) || !Path.IsPathRooted(c.WorkingDirectory) || !Path.IsPathRooted(c.ArtifactDirectory) || c.Documents.Length == 0 || c.Documents.Length > 8 || c.Arguments.Length > 16 || c.MaximumActions < 1 || c.MaximumActions > 1024 || c.LifetimeSeconds < 1 || c.LifetimeSeconds > 295 || c.Actions.Any(a => !Adapter.ActionSet.Contains(a))) throw new Denied("invalid_configuration");
            c.TextControlClasses=Json.Strings(map,"text_control_classes");
            c.SaveDestinations=Json.Strings(map,"save_destinations");
            if(c.SaveDestinations.Length>8 || c.SaveDestinations.Any(d=>!c.Documents.Contains(d))) throw new Denied("save_destination_scope_invalid");
            c.ActionLedger=Json.Text(map,"action_ledger"); c.CampaignId=Json.Text(map,"campaign_id");
            Guid campaign; if(!Guid.TryParse(c.CampaignId,out campaign) || !Path.IsPathRooted(c.ActionLedger) || !File.Exists(c.ActionLedger)) throw new Denied("explicit_campaign_ledger_required");
            CheckPath(c.ActionLedger);
            if(c.TextControlClasses.Length>8) throw new Denied("text_control_class_capacity");
            if (!Directory.Exists(c.WorkingDirectory) || !Directory.Exists(c.ArtifactDirectory)) throw new Denied("scope_unavailable");
            foreach (string doc in c.Documents) {
                if (Path.GetFileName(doc) != doc || doc.Contains(":") || doc.StartsWith(".") || !doc.EndsWith(".txt", StringComparison.OrdinalIgnoreCase)) throw new Denied("document_scope_invalid");
            }
            CheckPath(c.Program); CheckPath(c.WorkingDirectory); CheckPath(c.ArtifactDirectory);
            return c;
        }
        internal static void CheckPath(string path)
        {
            for (var p = new FileInfo(Path.GetFullPath(path)); p != null; p = p.Directory == null ? null : new FileInfo(p.Directory.FullName)) {
                if ((File.GetAttributes(p.FullName) & FileAttributes.ReparsePoint) != 0) throw new Denied("reparse_scope_denied");
            }
        }
        internal string Document(string leaf) { if (!Documents.Contains(leaf)) throw new Denied("document_not_owner_admitted"); var path=Path.Combine(WorkingDirectory,leaf); if (File.Exists(path)) CheckPath(path); return path; }
    }

    internal static class Json
    {
        internal static readonly JavaScriptSerializer Serializer = new JavaScriptSerializer { MaxJsonLength=262144, RecursionLimit=24 };
        internal static Dictionary<string,object> Read(string text) { var result=Serializer.DeserializeObject(text) as Dictionary<string,object>; if (result == null) throw new Denied("object_required"); return result; }
        internal static string Write(object value) { return Serializer.Serialize(value); }
        internal static void Keys(Dictionary<string,object> obj, params string[] allowed) { if (obj.Keys.Any(k => !allowed.Contains(k))) throw new Denied("unknown_argument"); }
        internal static string Text(Dictionary<string,object> obj, string key) { object value; if (!obj.TryGetValue(key,out value) || !(value is string)) throw new Denied("string_required"); return (string)value; }
        internal static int Number(Dictionary<string,object> obj, string key) { object value; if (!obj.TryGetValue(key,out value) || !(value is int)) throw new Denied("integer_required"); return (int)value; }
        internal static bool Boolean(Dictionary<string,object> obj, string key) { object value; if (!obj.TryGetValue(key,out value) || !(value is bool)) throw new Denied("boolean_required"); return (bool)value; }
        internal static string[] Strings(Dictionary<string,object> obj, string key) { object value; if (!obj.TryGetValue(key,out value) || !(value is object[])) throw new Denied("array_required"); return ((object[])value).Select(v => { if (!(v is string)) throw new Denied("string_array_required"); return (string)v; }).ToArray(); }
    }

    internal sealed class Observation
    {
        internal AutomationElement Element;
        internal int[] RuntimeId;
        internal int Generation;
        internal IntPtr Window;
        internal System.Windows.Rect Geometry;
        internal string Fingerprint;
        internal long StructureEpoch;
    }

    internal sealed class ActionLedger : IDisposable
    {
        private readonly FileStream file;
        private readonly byte[] identity;
        internal int Attempts { get; private set; }
        internal int ApiReturns { get; private set; }
        internal static byte[] Identity(Config c) { using(var hash=SHA256.Create()) return hash.ComputeHash(Encoding.UTF8.GetBytes("NEXT02/actions/v1\n"+c.CampaignId+"\n"+c.Sha256+"\n"+Path.GetFullPath(c.WorkingDirectory))); }
        internal ActionLedger(Config c)
        {
            identity=Identity(c);
            file=new FileStream(c.ActionLedger,FileMode.Open,FileAccess.ReadWrite,FileShare.None);
            if(file.Length!=40) { file.Dispose(); throw new Denied("action_ledger_invalid_unknown_is_not_zero"); }
            var bytes=new byte[40]; if(file.Read(bytes,0,40)!=40 || !bytes.Take(32).SequenceEqual(identity)) { file.Dispose(); throw new Denied("action_ledger_scope_changed"); }
            Attempts=BitConverter.ToInt32(bytes,32); ApiReturns=BitConverter.ToInt32(bytes,36);
            if(Attempts<0 || Attempts>1024 || ApiReturns<0 || ApiReturns>Attempts) { file.Dispose(); throw new Denied("action_accounting_invalid"); }
        }
        private void Persist()
        {
            var bytes=new byte[40]; identity.CopyTo(bytes,0); BitConverter.GetBytes(Attempts).CopyTo(bytes,32);BitConverter.GetBytes(ApiReturns).CopyTo(bytes,36);
            file.Position=0;file.Write(bytes,0,40);file.Flush(true);
        }
        internal void Admit(int maximum) { if(Attempts>=maximum) throw new Denied("primitive_action_limit"); Attempts++;Persist(); }
        internal void ReserveBatch(int count,int maximum) { if(count<0 || Attempts+count>maximum) throw new Denied("primitive_action_limit_no_partial_batch"); Attempts+=count;Persist(); }
        internal void Returned(bool persist=true) { ApiReturns++;if(persist) Persist(); }
        internal void CommitReturns() { Persist(); }
        public void Dispose() { file.Dispose(); }
    }

    internal sealed class Adapter : IDisposable
    {
        internal static readonly string[] ActionSet = { "invoke", "expand", "collapse", "select", "toggle", "set_value", "select_all", "select_text", "replace_selection", "scroll_down", "scroll_up" };
        private readonly Config config;
        private readonly FileStream executable;
        private readonly DateTime deadline;
        private Process process;
        private long processStart;
        private IntPtr root;
        private int session;
        private string desktop;
        private uint lastInput;
        private int generation;
        private readonly ActionLedger ledger;
        private readonly object disposal=new object();
        private bool disposed;
        private bool sensitiveSurface;
        private bool observationIncomplete;
        private long structureEpoch;
        private AutomationElement applicationRoot;
        private StructureChangedEventHandler structureHandler;
        private AutomationEventHandler closedHandler;
        private readonly string context = Guid.NewGuid().ToString();
        private readonly Dictionary<string,Observation> observations = new Dictionary<string,Observation>();
        internal volatile bool Stopped;
        internal volatile bool Paused;
        private volatile bool resumeRequested;
        internal int? LaunchExitCode;
        internal int Actions { get { return ledger.Attempts; } }
        internal bool DeadlineReached { get { return DateTime.UtcNow>=deadline; } }
        internal Adapter(Config c)
        {
            config=c; deadline=DateTime.UtcNow.AddSeconds(c.LifetimeSeconds);
            executable=new FileStream(c.Program,FileMode.Open,FileAccess.Read,FileShare.Read);
            if (Hash(executable) != c.Sha256) { executable.Dispose(); throw new Denied("application_identity_changed"); }
            executable.Position=0;
            ledger=new ActionLedger(c);
        }
        internal static string Hash(Stream stream) { using(var h=SHA256.Create()) return BitConverter.ToString(h.ComputeHash(stream)).Replace("-", "").ToLowerInvariant(); }
        private void Alive()
        {
            if (Stopped || DateTime.UtcNow >= deadline) throw new Denied("cancelled_or_deadline");
            if (resumeRequested) {
                if(root==IntPtr.Zero || Native.GetForegroundWindow()!=root) throw new Denied("owner_resume_waiting_for_owned_focus");
                resumeRequested=false; lastInput=Native.InputTime(); Paused=false; observations.Clear(); generation++;
            }
            if (Paused) throw new Denied("owner_takeover_paused");
            if (process == null || process.HasExited || process.StartTime.ToFileTimeUtc() != processStart || process.SessionId != session || !Native.IsWindow(root)) throw new Denied("application_identity_stale");
            uint pid; Native.GetWindowThreadProcessId(root,out pid); if (pid != process.Id) throw new Denied("window_identity_stale");
            if (desktop != Native.InteractiveDesktop() || desktop != Native.DesktopName(Native.GetThreadDesktop(Native.GetCurrentThreadId()))) throw new Denied("desktop_changed_or_secure");
            if (!Native.IsWindowVisible(root) || Native.IsIconic(root)) throw new Denied("window_unavailable");
            if (Native.InputTime() != lastInput) { Paused=true; throw new Denied("owner_input_takeover"); }
        }
        private bool OwnedWindow(IntPtr window)
        {
            uint pid; Native.GetWindowThreadProcessId(window,out pid);
            return pid == process.Id && Native.GetAncestor(window,3) == root;
        }
        private void Foreground()
        {
            Alive();
            var window=Native.GetForegroundWindow();
            if (!OwnedWindow(window)) { Paused=true; throw new Denied("foreground_takeover_paused"); }
            if (window != root) {
                var e=AutomationElement.FromHandle(window);
                if (!config.Dialogs.Contains(e.Current.Name)) { Paused=true; throw new Denied("unexpected_owned_dialog"); }
            }
        }
        internal object Launch(Dictionary<string,object> args)
        {
            Json.Keys(args,"document");
            if (Stopped || DateTime.UtcNow >= deadline) throw new Denied("cancelled_or_deadline");
            if (process != null) throw new Denied("already_launched_use_current_observation");
            string doc=config.Document(Json.Text(args,"document"));
            if (!File.Exists(doc)) throw new Denied("source_document_unavailable");
            desktop=Native.InteractiveDesktop();
            if (desktop != "Default") throw new Denied("nondefault_desktop_denied");
            foreach(string arg in config.Arguments) if (arg.Contains("\"") || arg.IndexOfAny(new[]{'\r','\n','\0'}) >= 0) throw new Denied("launch_arguments_invalid");
            var start=new ProcessStartInfo(config.Program, string.Join(" ",config.Arguments.Select(Quote).ToArray())+" "+Quote(doc)) { UseShellExecute=false, WorkingDirectory=config.WorkingDirectory };
            start.EnvironmentVariables.Clear();
            foreach(var key in new[]{"SystemRoot","WINDIR","TEMP","TMP","LOCALAPPDATA"}) { var value=Environment.GetEnvironmentVariable(key); if (value != null) start.EnvironmentVariables[key]=value; }
            lastInput=Native.InputTime();
            ledger.Admit(config.MaximumActions);
            process=Process.Start(start); processStart=process.StartTime.ToFileTimeUtc(); session=process.SessionId;
            ledger.Returned();
            if(session!=Process.GetCurrentProcess().SessionId || !Native.OrdinaryToken(process.Handle)) { Stopped=true;throw new Denied("application_elevation_uiaccess_or_session_denied"); }
            process.WaitForInputIdle(2000);
            var ready=Stopwatch.StartNew();
            do { process.Refresh(); root=process.MainWindowHandle; if(root!=IntPtr.Zero || process.HasExited) break; Thread.Sleep(25); } while(ready.ElapsedMilliseconds<1500 && !Stopped);
            if (root == IntPtr.Zero) {
                if(process.HasExited) { LaunchExitCode=process.ExitCode;throw new Denied("owned_application_exited_before_window"); }
                throw new Denied("owned_window_unavailable_no_attach");
            }
            if(Native.InputTime()!=lastInput) { Paused=true; throw new Denied("owner_input_during_launch_paused"); }
            if(Native.GetForegroundWindow()!=root) {
                ledger.Admit(config.MaximumActions);
                try { AutomationElement.FromHandle(root).SetFocus(); }
                catch(InvalidOperationException) { Paused=true; throw new Denied("initial_focus_unavailable_owner_resume_required"); }
                if(Native.GetForegroundWindow()!=root) { Paused=true; throw new Denied("initial_focus_unavailable_owner_resume_required"); }
                ledger.Returned();
                // UIA's initial focus implementation can update last-input state.
                // This is one explicit owned launch focus, never a retry loop.
                // Later owner input is checked before every action; targeted
                // messages do not inject global keyboard input or reset it.
                lastInput=Native.InputTime();
            }
            Alive(); generation++;
            applicationRoot=AutomationElement.FromHandle(root);
            structureHandler=(sender,change)=>Interlocked.Increment(ref structureEpoch);
            closedHandler=(sender,change)=>{Stopped=true;Interlocked.Increment(ref structureEpoch);};
            Automation.AddStructureChangedEventHandler(applicationRoot,TreeScope.Subtree,structureHandler);
            Automation.AddAutomationEventHandler(WindowPattern.WindowClosedEvent,applicationRoot,TreeScope.Element,closedHandler);
            return new { context_id=context, launched=true, process_id=process.Id, process_creation=processStart, generation=generation, capture_origin="https://windows.argentum.invalid", admitted_documents=config.Documents.Select(d=>new { name=d,path=config.Document(d) }).ToArray(), boundary="current_user_application_policy_not_os_filesystem_sandbox" };
        }
        private static string Quote(string value) { if (value.Contains("\"") || value.EndsWith("\\")) throw new Denied("invalid_launch_value"); return "\""+value+"\""; }
        private static string Fingerprint(AutomationElement element)
        {
            var c=element.Current;
            if (c.IsPassword) throw new Denied("password_control_denied");
            return c.Name+"\n"+c.AutomationId+"\n"+c.ClassName+"\n"+c.ControlType.Id+"\n"+c.IsEnabled+"\n"+c.IsOffscreen;
        }
        internal object Observe(Dictionary<string,object> args)
        {
            Json.Keys(args,"text_limit","offset","element_limit"); Foreground();
            int limit=args.ContainsKey("text_limit") ? Json.Number(args,"text_limit") : 8000;
            if (limit < 256 || limit > 12000) throw new Denied("text_limit_invalid");
            int offset=args.ContainsKey("offset") ? Json.Number(args,"offset") : 0;
            int elementLimit=args.ContainsKey("element_limit") ? Json.Number(args,"element_limit") : 16;
            if(offset<0 || offset>127 || elementLimit<1 || elementLimit>32) throw new Denied("observation_page_invalid");
            observations.Clear(); sensitiveSurface=false; generation++;
            var current=Native.GetForegroundWindow(); var anchor=AutomationElement.FromHandle(current);
            var queue=new Queue<AutomationElement>(); queue.Enqueue(anchor);
            var result=new List<Dictionary<string,object>>(); int examined=0, textBudget=limit; bool traversalTruncated=false;
            while(queue.Count>0 && examined++ < 256 && result.Count < 128) {
                Alive(); var e=queue.Dequeue(); var c=e.Current;
                if (c.ProcessId != process.Id) continue;
                if (c.IsPassword) { sensitiveSurface=true; continue; }
                var walker=TreeWalker.ControlViewWalker; var child=walker.GetFirstChild(e); int siblings=0;
                while(child != null && siblings++ < 128) { queue.Enqueue(child); child=walker.GetNextSibling(child); }
                if(child!=null) traversalTruncated=true;
                if (c.IsOffscreen || !c.IsEnabled || c.BoundingRectangle.IsEmpty) continue;
                string name=c.Name ?? ""; if (name.Length>512) name=name.Substring(0,512);
                string text=""; object pattern;
                if (textBudget>0 && e.TryGetCurrentPattern(TextPattern.Pattern,out pattern)) text=((TextPattern)pattern).DocumentRange.GetText(Math.Min(8000,textBudget));
                else if (textBudget>0 && e.TryGetCurrentPattern(ValuePattern.Pattern,out pattern)) text=((ValuePattern)pattern).Current.Value;
                if (text.Length>textBudget) text=text.Substring(0,textBudget); textBudget-=text.Length;
                var supported=e.GetSupportedPatterns().Select(p=>p.ProgrammaticName).ToArray();
                string reference=Guid.NewGuid().ToString("N");
                observations.Add(reference,new Observation { Element=e, RuntimeId=e.GetRuntimeId(), Generation=generation, Window=current, Geometry=c.BoundingRectangle, Fingerprint=Fingerprint(e), StructureEpoch=Interlocked.Read(ref structureEpoch) });
                result.Add(new Dictionary<string,object> { {"reference",reference},{"name",name},{"role",c.ControlType.ProgrammaticName},{"control_class",c.ClassName},{"patterns",supported},{"text",text},{"focused",c.HasKeyboardFocus},{"geometry",new { x=c.BoundingRectangle.X,y=c.BoundingRectangle.Y,width=c.BoundingRectangle.Width,height=c.BoundingRectangle.Height }} });
            }
            observationIncomplete=queue.Count>0 || traversalTruncated;
            var page=result.Skip(offset).Take(elementLimit).ToList();
            bool textTruncated=false;
            Func<object> payload=()=>new { context_id=context,generation=generation, elements=page, offset=offset, next_offset=offset+page.Count<result.Count?(int?)(offset+page.Count):null, total_elements=result.Count, truncated=observationIncomplete || offset+page.Count<result.Count || textTruncated, scan_truncated=observationIncomplete, text_truncated=textTruncated, untrusted_application_data=true, actions=ledger.Attempts, api_returned_actions=ledger.ApiReturns, cumulative_campaign_actions=true };
            // Bound the actual duplicated MCP envelope, not only text. A large
            // dialog is paged without dropping its filename/action controls.
            // The full bounded traversal still checks password/capture scope.
            while(EnvelopeBytes(payload())>20*1024) {
                if(page.Count>1) { page.RemoveAt(page.Count-1); continue; }
                if(page.Count==0) throw new Denied("native_result_capacity_narrow_observation");
                string text=(string)page[0]["text"];
                if(text.Length==0) throw new Denied("native_result_capacity_narrow_observation");
                int length=text.Length/2;
                if(length>0 && char.IsHighSurrogate(text[length-1])) length--;
                page[0]["text"]=text.Substring(0,length); page[0]["text_truncated"]=true; textTruncated=true;
            }
            // Retain the bounded current private observations for complete
            // capture/save checks, including fields on another output page.
            // No hidden reference is disclosed; the next observation expires all.
            return payload();
        }
        private static int EnvelopeBytes(object value) { return Encoding.UTF8.GetByteCount(Json.Write(new { isError=false,content=new[]{new{type="text",text=Json.Write(value)}},structuredContent=value })); }
        private Observation Target(Dictionary<string,object> args)
        {
            Foreground(); Observation o;
            if (Json.Number(args,"generation") != generation || !observations.TryGetValue(Json.Text(args,"reference"),out o) || o.Generation != generation) throw new Denied("stale_observation_reobserve");
            var c=o.Element.Current;
            if (o.StructureEpoch!=Interlocked.Read(ref structureEpoch) || c.ProcessId != process.Id || c.IsPassword || !c.IsEnabled || c.IsOffscreen || !o.RuntimeId.SequenceEqual(o.Element.GetRuntimeId()) || o.Geometry != c.BoundingRectangle || o.Fingerprint != Fingerprint(o.Element) || o.Window != Native.GetForegroundWindow() || !OwnedWindow(o.Window)) throw new Denied("stale_element_or_geometry_reobserve");
            return o;
        }
        private void AdmitAction()
        {
            Foreground(); ledger.Admit(config.MaximumActions);
        }
        private bool FileNameField(AutomationElement element,IntPtr window)
        {
            var field=element.Current;
            var hwnd=new IntPtr(field.NativeWindowHandle); uint pid;
            Native.GetWindowThreadProcessId(hwnd,out pid);
            if(field.ControlType!=ControlType.Edit || hwnd==IntPtr.Zero || pid!=process.Id || Native.GetAncestor(hwnd,2)!=window) return false;
            for(int depth=0;depth<3 && element!=null;depth++) {
                var c=element.Current;
                if(c.ProcessId!=process.Id) return false;
                if(c.AutomationId=="1148" || c.Name.StartsWith("File name",StringComparison.OrdinalIgnoreCase)) return true;
                element=TreeWalker.ControlViewWalker.GetParent(element);
            }
            return false;
        }
        private bool FileDialog(IntPtr window)
        {
            if(window==root) return false;
            var name=AutomationElement.FromHandle(window).Current.Name;
            return name=="Save As" || name=="Open";
        }
        private void CheckFileDialogCommit(IntPtr window)
        {
            var allowed=AutomationElement.FromHandle(window).Current.Name=="Save As" ? config.SaveDestinations : config.Documents;
            bool selected=false;
            foreach(var observation in observations.Values.Where(o=>o.Window==window)) {
                object value;
                if(FileNameField(observation.Element,window) && observation.Element.TryGetCurrentPattern(ValuePattern.Pattern,out value) && !((ValuePattern)value).Current.IsReadOnly) {
                    var current=observation.Element.Current;
                    if(!current.IsEnabled || current.IsOffscreen || current.IsPassword) continue;
                    string path=((ValuePattern)value).Current.Value;
                    if(allowed.Any(d=>path==config.Document(d))) selected=true;
                }
            }
            if(!selected) throw new Denied("file_dialog_commit_requires_exact_admitted_path");
        }
        internal object Act(Dictionary<string,object> args)
        {
            Json.Keys(args,"generation","reference","action","value");
            string action=Json.Text(args,"action");
            if (!config.Actions.Contains(action)) throw new Denied("action_not_owner_enabled");
            var o=Target(args); object pattern;
            if (new[]{"invoke","expand","collapse","select","toggle"}.Contains(action) && !config.InvokableNames.Contains(o.Element.Current.Name)) throw new Denied("control_action_not_owner_admitted");
            string value=args.ContainsKey("value") ? Json.Text(args,"value") : null;
            if (value != null && (value.Length > 8000 || value.Contains("\0"))) throw new Denied("text_capacity_or_nul");
            if ((action=="set_value" || action=="select_text" || action=="replace_selection") != (value!=null)) throw new Denied("action_value_mismatch");
            if(action=="invoke" && o.Window==root && o.Element.Current.Name=="Save") throw new Denied("in_place_source_save_not_admitted_use_save_as");
            // File-dialog destinations are exact owner-admitted documents, never
            // arbitrary filesystem paths supplied by model or application text.
            if (FileDialog(o.Window) && action=="set_value") {
                var allowed=AutomationElement.FromHandle(o.Window).Current.Name=="Save As" ? config.SaveDestinations : config.Documents;
                if(!FileNameField(o.Element,o.Window) || !allowed.Any(d=>value==config.Document(d))) throw new Denied("file_dialog_destination_denied_use_exact_admitted_absolute_path");
            }
            if(FileDialog(o.Window) && action=="invoke" && new[]{"Save","Open"}.Contains(o.Element.Current.Name)) CheckFileDialogCommit(o.Window);
            try {
                if (action=="invoke" && o.Element.TryGetCurrentPattern(InvokePattern.Pattern,out pattern)) { AdmitAction(); ((InvokePattern)pattern).Invoke(); }
                else if (action=="expand" && o.Element.TryGetCurrentPattern(ExpandCollapsePattern.Pattern,out pattern)) { AdmitAction(); ((ExpandCollapsePattern)pattern).Expand(); }
                else if (action=="collapse" && o.Element.TryGetCurrentPattern(ExpandCollapsePattern.Pattern,out pattern)) { AdmitAction(); ((ExpandCollapsePattern)pattern).Collapse(); }
                else if (action=="select" && o.Element.TryGetCurrentPattern(SelectionItemPattern.Pattern,out pattern)) { AdmitAction(); ((SelectionItemPattern)pattern).Select(); }
                else if (action=="toggle" && o.Element.TryGetCurrentPattern(TogglePattern.Pattern,out pattern)) { AdmitAction(); ((TogglePattern)pattern).Toggle(); }
                else if (action=="set_value" && o.Element.TryGetCurrentPattern(ValuePattern.Pattern,out pattern) && !((ValuePattern)pattern).Current.IsReadOnly) { AdmitAction(); ((ValuePattern)pattern).SetValue(value); }
                else if ((action=="select_all" || action=="select_text") && o.Element.TryGetCurrentPattern(TextPattern.Pattern,out pattern)) {
                    var range=((TextPattern)pattern).DocumentRange;
                    if (action=="select_text") { range=range.FindText(value,false,false); if (range==null) throw new Denied("text_not_found"); }
                    AdmitAction(); range.Select();
                }
                else if ((action=="scroll_down" || action=="scroll_up") && o.Element.TryGetCurrentPattern(ScrollPattern.Pattern,out pattern)) { AdmitAction(); ((ScrollPattern)pattern).Scroll(ScrollAmount.NoAmount,action=="scroll_down"?ScrollAmount.LargeIncrement:ScrollAmount.LargeDecrement); }
                else if (action=="replace_selection" && config.TargetedTextInput) {
                    // Text insertion uses LF as one native Enter character.
                    // Reject CR and non-text control characters, rather than
                    // silently interpreting them as editing/navigation keys.
                    if(value.Any(ch=>char.IsControl(ch) && ch!='\n' && ch!='\t')) throw new Denied("text_control_character_denied_use_lf_newlines");
                    for(int i=0;i<value.Length;i++) {
                        if(char.IsHighSurrogate(value[i])) { if(i+1>=value.Length || !char.IsLowSurrogate(value[++i])) throw new Denied("invalid_utf16_text"); }
                        else if(char.IsLowSurrogate(value[i])) throw new Denied("invalid_utf16_text");
                    }
                    var hwnd=new IntPtr(o.Element.Current.NativeWindowHandle);
                    if (o.Window!=root || hwnd==IntPtr.Zero || !OwnedWindow(Native.GetAncestor(hwnd,2)) || !o.Element.Current.HasKeyboardFocus || !Native.IsWindowEnabled(hwnd) || !config.TextControlClasses.Contains(o.Element.Current.ClassName)) throw new Denied("targeted_text_control_unavailable");
                    if (value.Length>256) throw new Denied("targeted_text_batch_capacity");
                    if (ledger.Attempts+value.Length>config.MaximumActions) throw new Denied("primitive_action_limit_no_partial_batch");
                    // Reserve the whole bounded batch before any input. Persist
                    // returned counts once at settlement rather than one HDD
                    // flush per character. Reservation is conservative: it is
                    // not a claim that every character was applied.
                    ledger.ReserveBatch(value.Length,config.MaximumActions);
                    try { foreach(char ch in value) {
                        Foreground(); if (o.StructureEpoch!=Interlocked.Read(ref structureEpoch) || !o.RuntimeId.SequenceEqual(o.Element.GetRuntimeId()) || !o.Element.Current.HasKeyboardFocus || !Native.IsWindow(hwnd)) throw new Denied("partial_text_effect_uncertain");
                        UIntPtr result;
                        if (Native.SendMessageTimeout(hwnd,0x0102,new UIntPtr(ch=='\n'?'\r':ch),IntPtr.Zero,2,250,out result)==IntPtr.Zero) { Stopped=true; throw new Denied("input_timeout_effect_uncertain"); }
                        ledger.Returned(false);
                    } } finally { ledger.CommitReturns(); }
                }
                else throw new Denied("pattern_unavailable_no_implicit_fallback");
                if(action!="replace_selection") ledger.Returned();
            } finally { generation++; observations.Clear(); }
            return new { context_id=context,api_returned=true, observed_outcome_verified=false, generation=generation, actions=ledger.Attempts, api_returned_actions=ledger.ApiReturns, cumulative_campaign_actions=true, reobserve_required=true };
        }
        internal object Capture(Dictionary<string,object> args)
        {
            Json.Keys(args,"generation"); Foreground();
            if (Json.Number(args,"generation")!=generation || observations.Count==0) throw new Denied("stale_image_observation");
            if(observationIncomplete || observations.Values.Any(o=>o.StructureEpoch!=Interlocked.Read(ref structureEpoch) || o.Fingerprint!=Fingerprint(o.Element))) throw new Denied("capture_observation_stale_or_incomplete");
            IntPtr window=Native.GetForegroundWindow(); uint affinity; int cloaked; Native.Rect rect;
            if (!Native.GetWindowDisplayAffinity(window,out affinity) || affinity!=0 || Native.DwmGetWindowAttribute(window,14,out cloaked,4)!=0 || cloaked!=0 || !Native.GetWindowRect(window,out rect)) throw new Denied("capture_scope_unavailable");
            if (sensitiveSurface) throw new Denied("password_surface_capture_denied");
            int width=rect.Right-rect.Left,height=rect.Bottom-rect.Top;
            if (width<1 || height<1 || width>2560 || height>1600) throw new Denied("capture_geometry_capacity");
            // PrintWindow renders a DPI-unaware application's logical surface.
            // Query that surface in its own DPI context, while retaining the
            // independent physical UIA/window geometry for explicit mapping.
            Native.Rect surface;
            var previousDpi=Native.SetThreadDpiAwarenessContext(Native.GetWindowDpiAwarenessContext(window));
            if(previousDpi==IntPtr.Zero) throw new Denied("capture_dpi_context_unavailable");
            try { if(!Native.GetWindowRect(window,out surface)) throw new Denied("capture_geometry_unavailable"); }
            finally { Native.SetThreadDpiAwarenessContext(previousDpi); }
            int surfaceWidth=surface.Right-surface.Left,surfaceHeight=surface.Bottom-surface.Top;
            if(surfaceWidth<1 || surfaceHeight<1 || surfaceWidth>2560 || surfaceHeight>1600) throw new Denied("capture_geometry_capacity");
            byte[] bytes;
            int imageWidth,imageHeight;
            using(var bitmap=new Bitmap(surfaceWidth,surfaceHeight)) {
                using(var graphics=Graphics.FromImage(bitmap)) { IntPtr dc=graphics.GetHdc(); try { if (!Native.PrintWindow(window,dc,2)) throw new Denied("capture_unavailable"); } finally { graphics.ReleaseHdc(dc); } }
                Foreground(); Native.Rect after; Native.GetWindowRect(window,out after);
                if (window!=Native.GetForegroundWindow() || rect.Left!=after.Left || rect.Top!=after.Top || rect.Right!=after.Right || rect.Bottom!=after.Bottom) throw new Denied("capture_geometry_changed");
                double scale=Math.Min(1,Math.Min(1200.0/surfaceWidth,800.0/surfaceHeight));
                imageWidth=(int)(surfaceWidth*scale);imageHeight=(int)(surfaceHeight*scale);
                using(var resized=new Bitmap(bitmap,new Size(imageWidth,imageHeight))) using(var stream=new MemoryStream()) { resized.Save(stream,ImageFormat.Png); bytes=stream.ToArray(); }
            }
            if (bytes.Length>128*1024) throw new Denied("image_byte_capacity");
            string artifact=Guid.NewGuid()+".png", sha;
            using(var stream=new MemoryStream(bytes)) sha=Hash(stream);
            using(var output=new FileStream(Path.Combine(config.ArtifactDirectory,artifact),FileMode.CreateNew,FileAccess.Write,FileShare.None)) output.Write(bytes,0,bytes.Length);
            return new { context_id=context,origin="https://windows.argentum.invalid",artifact=artifact,sha256=sha,bytes=bytes.Length,generation=generation,captured_at=DateTime.UtcNow.ToString("o"),geometry=new { x=rect.Left,y=rect.Top,width=width,height=height,dpi=Native.GetDpiForWindow(window) },image_geometry=new {width=imageWidth,height=imageHeight},scope="admitted_window_only" };
        }
        internal object Call(string name, Dictionary<string,object> args)
        {
            if (name=="desktop_launch") return Launch(args);
            if (name=="desktop_observe") return Observe(args);
            if (name=="desktop_action") return Act(args);
            if (name=="desktop_screenshot") return Capture(args);
            if (name=="desktop_close") { Json.Keys(args); int attempts=ledger.Attempts,returns=ledger.ApiReturns; Dispose(); return new { closed=true,actions=attempts,api_returned_actions=returns,cumulative_campaign_actions=true }; }
            throw new Denied("unknown_tool");
        }
        public void Dispose()
        {
            // The UI watchdog and worker may request teardown concurrently.
            // Only one owner disposes the pinned process and ledger; a window
            // close event cannot null the process beneath another cleanup.
            lock(disposal) {
                if(disposed) return;
                disposed=true;Stopped=true;
                var owned=process;process=null;
                try {
                    if(owned!=null && !owned.HasExited) {
                        if(owned.StartTime.ToFileTimeUtc()!=processStart) throw new Denied("owned_cleanup_identity_uncertain");
                        owned.Kill();
                        if(!owned.WaitForExit(2000)) throw new Denied("owned_cleanup_timeout_uncertain");
                    }
                } finally {
                    if(owned!=null) owned.Dispose();
                    executable.Dispose();ledger.Dispose();
                }
            }
        }
        internal void OwnerResume() { resumeRequested=true; }
        internal void RemoveObservers()
        {
            if(applicationRoot==null) return;
            if(structureHandler!=null) Automation.RemoveStructureChangedEventHandler(applicationRoot,structureHandler);
            if(closedHandler!=null) Automation.RemoveAutomationEventHandler(WindowPattern.WindowClosedEvent,applicationRoot,closedHandler);
        }
    }

    internal sealed class UiaDispatcher : IDisposable
    {
        private sealed class Work { internal Func<object> Run; internal object Result; internal Exception Failure; internal readonly ManualResetEventSlim Done=new ManualResetEventSlim(); }
        private readonly BlockingCollection<Work> queue=new BlockingCollection<Work>(1);
        private readonly Thread thread;
        private readonly Adapter adapter;
        internal UiaDispatcher(Adapter value)
        {
            adapter=value;
            thread=new Thread(()=> { try { foreach(var work in queue.GetConsumingEnumerable()) { try { if(adapter.Stopped) throw new Denied("cancelled"); work.Result=work.Run(); } catch(Exception e) { work.Failure=e; } finally { work.Done.Set(); } } } finally { try { adapter.RemoveObservers(); } catch(Exception) {} } }) { IsBackground=true };
            thread.SetApartmentState(ApartmentState.MTA); thread.Start();
        }
        internal object Invoke(string name,Dictionary<string,object> args)
        {
            if(adapter.Stopped) throw new Denied("connection_retired");
            var work=new Work { Run=()=>adapter.Call(name,args) }; queue.Add(work);
            if(!work.Done.Wait(5000)) { adapter.Stopped=true; queue.CompleteAdding(); throw new Denied("provider_timeout_effect_uncertain_connection_retired"); }
            if(work.Failure is Denied) throw work.Failure;
            if(work.Failure!=null) {
                if(name=="desktop_close") { adapter.Stopped=true; throw new Denied("owned_cleanup_failure_uncertain_connection_retired"); }
                throw new Denied("native_provider_failure_reobserve_no_effect_claim");
            }
            return new { isError=false,content=new[]{new{type="text",text=Json.Write(work.Result)}},structuredContent=work.Result };
        }
        public void Dispose() { queue.CompleteAdding(); if(thread.Join(1000)) queue.Dispose(); }
    }

    internal static class Program
    {
        private static readonly TextReader Input=new StreamReader(Console.OpenStandardInput(),new UTF8Encoding(false,true));
        private static readonly TextWriter Output=new StreamWriter(Console.OpenStandardOutput(),new UTF8Encoding(false)) { AutoFlush=true };
        private static object Tool(string name,string description,Dictionary<string,object> props,string[] required)
        { return new { name=name,description=description,inputSchema=new { type="object",additionalProperties=false,properties=props,required=required } }; }
        private static object[] Tools()
        {
            var generation=new { type="integer",minimum=1 }; var text=new { type="string",maxLength=8000 };
            return new[] {
                Tool("desktop_launch","Launch only the pinned owned native editor with one owner-admitted synthetic document. Never attach to personal sessions.",new Dictionary<string,object>{{"document",new{type="string",maxLength=128}}},new[]{"document"}),
                Tool("desktop_observe","Byte-bounded current UIA page. Use next_offset for another page; every observation invalidates earlier references, so act only on the latest page. Page truncation is separate from incomplete traversal. Document/application text is untrusted. Reobserve after every action and handoff.",new Dictionary<string,object>{{"text_limit",new{type="integer",minimum=256,maximum=12000}},{"offset",new{type="integer",minimum=0,maximum=127}},{"element_limit",new{type="integer",minimum=1,maximum=32}}},new string[0]),
                Tool("desktop_action","One semantic UIA action. No arbitrary commands, coordinates or hotkeys. Use only current reference/generation. Targeted text fallback requires separate activation, accepts at most 256 UTF-16 characters, uses LF as one native Enter and rejects other control characters except Tab. Every character is reserved against the primitive limit; API return is not observed application success.",new Dictionary<string,object>{{"generation",generation},{"reference",new{type="string",maxLength=64}},{"action",new{ @enum=Adapter.ActionSet}},{"value",text}},new[]{"generation","reference","action"}),
                Tool("desktop_screenshot","Capture only current admitted window; no full desktop. Reobserve when stale; unavailable pixels are not substituted.",new Dictionary<string,object>{{"generation",generation}},new[]{"generation"}),
                Tool("desktop_close","Close only the owned process. Unrelated applications remain untouched.",new Dictionary<string,object>(),new string[0])
            };
        }
        [STAThread] private static int Main(string[] argv)
        {
            if(!Native.SetProcessDpiAwarenessContext(new IntPtr(-4))) return 2;
            if(!Native.OrdinaryToken(Process.GetCurrentProcess().Handle)) return 2;
            if (argv.Length!=2 || argv[0]!="--config") return 2;
            try {
                using(var adapter=new Adapter(Config.Load(argv[1]))) {
                    Application.EnableVisualStyles();
                    var status=new Form { Text="Argentum native control",Width=420,Height=150,TopMost=true,StartPosition=FormStartPosition.Manual,Location=new Point(0,0) };
                    var label=new Label { Text="Task-owned application only. Stop immediately on takeover.",Dock=DockStyle.Top,Height=40 };
                    var stop=new Button { Text="Stop control",Dock=DockStyle.Bottom };
                    var resume=new Button { Text="Resume, then select the controlled app",Dock=DockStyle.Bottom };
                    status.Controls.Add(label); status.Controls.Add(stop); status.Controls.Add(resume);
                    stop.Click+=(s,e)=>{adapter.Stopped=true;status.Close();};
                    resume.Click+=(s,e)=>adapter.OwnerResume();
                    var watchdog=new System.Windows.Forms.Timer { Interval=100 };
                    watchdog.Tick+=(s,e)=>{label.Text=adapter.Paused?"Paused after owner input. No further actions admitted.":"Active native control. Primitive actions: "+adapter.Actions; if (adapter.Stopped || adapter.DeadlineReached) status.Close();};
                    var server=new Thread(()=>Serve(adapter,status)) { IsBackground=true }; server.SetApartmentState(ApartmentState.MTA);
                    status.Shown+=(s,e)=>{watchdog.Start();server.Start();};
                    Application.Run(status); adapter.Dispose();
                    if (!server.Join(1000)) Environment.Exit(3);
                }
                return 0;
            } catch(Exception) { Console.Error.WriteLine("desktop_adapter_start_rejected"); return 2; }
        }
        private static void Serve(Adapter adapter,Form status)
        {
            try { using(var dispatcher=new UiaDispatcher(adapter)) {
                while(!adapter.Stopped) {
                    string line=ReadLineBounded(); if (line==null) break;
                    Dictionary<string,object> request; object id=null;
                    try {
                        request=Json.Read(line); Json.Keys(request,"jsonrpc","id","method","params");
                        if (request.ContainsKey("id")) id=request["id"];
                        string method=Json.Text(request,"method"); object result;
                        if (method=="server/discover") result=new Dictionary<string,object> {
                            {"resultType","complete"},{"supportedVersions",new[]{"2026-07-28","2025-11-25"}},
                            {"capabilities",new {tools=new{}}},{"ttlMs",0},{"cacheScope","private"},
                            {"_meta",new Dictionary<string,object>{{"io.modelcontextprotocol/serverInfo",new{name="argentum-windows-desktop",version="0.1.0"}}}}
                        };
                        else if (method=="initialize") result=new { protocolVersion="2025-11-25",capabilities=new { tools=new{} },serverInfo=new{name="argentum-windows-desktop",version="0.1.0"} };
                        else if (method=="tools/list") result=new { tools=Tools() };
                        else if (method=="ping") result=new{};
                        else if (method.StartsWith("notifications/")) continue;
                        else if (method=="tools/call") {
                            var parameters=request["params"] as Dictionary<string,object>; if (parameters==null) throw new Denied("call_params_invalid");
                            Json.Keys(parameters,"name","arguments","_meta");
                            var args=parameters["arguments"] as Dictionary<string,object>; if(args==null) throw new Denied("arguments_invalid");
                            string name=Json.Text(parameters,"name");
                            result=dispatcher.Invoke(name,args);
                        } else throw new Denied("unsupported_method");
                        if(Encoding.UTF8.GetByteCount(Json.Write(result))>24*1024) throw new Denied("native_result_capacity_narrow_observation");
                        if(id!=null) Output.WriteLine(Json.Write(new { jsonrpc="2.0",id=id,result=result }));
                    } catch(Denied denial) {
                        if(id!=null) Output.WriteLine(Json.Write(new {jsonrpc="2.0",id=id,result=new {isError=true,content=new[]{new{type="text",text=denial.Message}},structuredContent=new { reason=denial.Message,api_outcome="not_inferred",actions=adapter.Actions,application_exit_code=adapter.LaunchExitCode }}}));
                    }
                    Output.Flush();
                }
            }
            } catch(Exception) { Console.Error.WriteLine("desktop_transport_unavailable"); }
            finally { adapter.Stopped=true; if (!status.IsDisposed) status.BeginInvoke(new Action(()=>status.Close())); }
        }
        private static string ReadLineBounded()
        {
            var b=new StringBuilder(); int ch;
            while((ch=Input.Read())!=-1) { if(ch=='\n') return b.ToString(); if(b.Length>=262144) throw new Denied("frame_capacity"); if(ch!='\r') b.Append((char)ch); }
            return b.Length==0?null:b.ToString();
        }
    }
}
