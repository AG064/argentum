// Offline counter controls. No application launch, input, capture or provider.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Linq;
using Argentum.WindowsDesktop;

internal static class TestLedger
{
    private static void Require(bool value) { if(!value) throw new Exception("control_failed"); }
    private static void Reject(Action action,string reason)
    {
        try { action(); } catch(Denied error) { Require(error.Message==reason); return; }
        throw new Exception("expected_rejection_missing");
    }
    private static void Seed(Config c,int attempts,int returns)
    {
        var bytes=new byte[40]; ActionLedger.Identity(c).CopyTo(bytes,0);
        BitConverter.GetBytes(attempts).CopyTo(bytes,32);
        BitConverter.GetBytes(returns).CopyTo(bytes,36);
        File.WriteAllBytes(c.ActionLedger,bytes);
    }
    private static string Configuration(Config c,int maximum)
    {
        var map=new Dictionary<string,object> {
            {"program",c.Program},{"sha256",c.Sha256},{"working_directory",c.WorkingDirectory},
            {"artifact_directory",c.ArtifactDirectory},{"action_ledger",c.ActionLedger},
            {"campaign_id",c.CampaignId},{"arguments",new string[0]},
            {"documents",new[]{"input.txt"}},{"save_destinations",new string[0]},
            {"dialogs",new string[0]},{"actions",new string[0]},
            {"invokable_names",new string[0]},{"text_control_classes",new string[0]},
            {"targeted_text_input",false},{"maximum_actions",maximum},{"lifetime_seconds",295}
        };
        var path=Path.Combine(c.WorkingDirectory,"configuration.json");
        File.WriteAllText(path,Json.Write(map)); return path;
    }
    private static int Main(string[] args)
    {
        if(args.Length==4 && args[0]=="reload") {
            var c=Config.Load(args[1]);
            using(var ledger=new ActionLedger(c)) {
                Require(ledger.Attempts==int.Parse(args[2]) && ledger.ApiReturns==int.Parse(args[3]));
                Reject(()=>ledger.Admit(ledger.Attempts),"primitive_action_limit");
            }
            return 0;
        }
        Require(args.Length==1 && Path.IsPathRooted(args[0]) && !Directory.Exists(args[0]));
        Directory.CreateDirectory(args[0]);
        var cfg=new Config { Program=Assembly.GetExecutingAssembly().Location,Sha256=new string('0',64),
            WorkingDirectory=args[0],ArtifactDirectory=args[0],CampaignId=Guid.NewGuid().ToString(),
            ActionLedger=Path.Combine(args[0],"actions.bin") };
        Seed(cfg,1023,1000);
        Require(Config.Load(Configuration(cfg,1024)).MaximumActions==1024);
        Require(Config.Load(Configuration(cfg,1536)).MaximumActions==1536);
        Reject(()=>Config.Load(Configuration(cfg,1537)),"invalid_configuration");
        Reject(()=>Config.Load(Configuration(cfg,0)),"invalid_configuration");
        using(var ledger=new ActionLedger(cfg)) {
            Reject(()=>ledger.ReserveBatch(2,1024),"primitive_action_limit_no_partial_batch");
            Require(ledger.Attempts==1023 && ledger.ApiReturns==1000);
            ledger.ReserveBatch(2,1536);ledger.Returned(false);ledger.Returned(false);ledger.CommitReturns();
        }
        // A new process, not an in-process object reload, must retain >1024 use.
        var config=Configuration(cfg,1536);
        using(var child=Process.Start(new ProcessStartInfo(cfg.Program,
            "reload \""+config+"\" 1025 1002") { UseShellExecute=false,CreateNoWindow=true })) {
            Require(child.WaitForExit(10000) && child.ExitCode==0);
        }
        using(var ledger=new ActionLedger(Config.Load(Configuration(cfg,1024)))) {
            Require(ledger.Attempts==1025 && ledger.ApiReturns==1002);
            Reject(()=>ledger.Admit(1024),"primitive_action_limit");
        }
        Seed(cfg,1535,1535);
        using(var ledger=new ActionLedger(cfg)) {
            Reject(()=>ledger.ReserveBatch(2,1536),"primitive_action_limit_no_partial_batch");
            Require(ledger.Attempts==1535);ledger.Admit(1536);ledger.Returned();
            Reject(()=>ledger.Admit(1536),"primitive_action_limit");
        }
        using(var ledger=new ActionLedger(cfg)) Require(ledger.Attempts==1536 && ledger.ApiReturns==1536);
        Seed(cfg,1537,0);Reject(()=>{using(var ledger=new ActionLedger(cfg)) {}},"action_accounting_invalid");
        Seed(cfg,-1,0);Reject(()=>{using(var ledger=new ActionLedger(cfg)) {}},"action_accounting_invalid");
        Seed(cfg,1,2);Reject(()=>{using(var ledger=new ActionLedger(cfg)) {}},"action_accounting_invalid");
        Seed(cfg,742,490);cfg.CampaignId=Guid.NewGuid().ToString();
        Reject(()=>{using(var ledger=new ActionLedger(cfg)) {}},"action_ledger_scope_changed");
        cfg.Documents=new[]{"input.txt","output.txt","caf\u00e9.txt"};
        var tools=typeof(Argentum.WindowsDesktop.Program).GetMethod("Tools",BindingFlags.Static|BindingFlags.NonPublic);
        var advertised=(object[])Json.Serializer.DeserializeObject(Json.Write(tools.Invoke(null,new object[]{cfg.Documents})));
        var launch=advertised.Cast<Dictionary<string,object>>().Single(t=>(string)t["name"]=="desktop_launch");
        var schema=(Dictionary<string,object>)launch["inputSchema"];
        var properties=(Dictionary<string,object>)schema["properties"];
        var document=(Dictionary<string,object>)properties["document"];
        Require(((object[])document["enum"]).Cast<string>().SequenceEqual(cfg.Documents));
        Require((string)document["type"]=="string" && (int)document["maxLength"]==128);
        Require(cfg.Document("caf\u00e9.txt")==Path.Combine(cfg.WorkingDirectory,"caf\u00e9.txt"));
        Reject(()=>cfg.Document(Path.Combine(cfg.WorkingDirectory,"input.txt")),"document_not_owner_admitted");
        Reject(()=>cfg.Document("../input.txt"),"document_not_owner_admitted");
        Reject(()=>cfg.Document("other.txt"),"document_not_owner_admitted");
        Console.WriteLine("Passed: serialized launch choices exactly match admitted basenames; qualified and unknown documents remain denied.");
        Console.WriteLine("Passed: explicit limits, batch pre-admission, durable >1024 fresh-process reload, lowered-limit retention, 1536 boundary, invalid counters and scope denial. No native effects.");
        return 0;
    }
}
