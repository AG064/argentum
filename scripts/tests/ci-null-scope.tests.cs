// Synthetic state tests. No native NUL handle is opened or device ACL changed.
using System;
using System.Collections.Generic;
using System.Security.AccessControl;

public sealed class ArgentumCiFakeNull : IArgentumCiNullDevice {
    public string Identity { get; set; } = ArgentumCiNullPlan.DeviceName;
    public byte[] State;
    public int Writes;
    public bool ThrowAfterFirstWrite;
    public bool FailRestore;
    public bool Closed;
    public ArgentumCiFakeNull(byte[] state) { State = (byte[])state.Clone(); }
    public byte[] Read() { return (byte[])State.Clone(); }
    public void WriteDacl(byte[] value) {
        Writes++;
        if (FailRestore && Writes == 2) throw new InvalidOperationException("Injected restore failure");
        State = (byte[])value.Clone();
        if (ThrowAfterFirstWrite && Writes == 1) throw new InvalidOperationException("Injected write result loss");
    }
    public void Dispose() { Closed = true; }
}

public static class ArgentumCiNullTests {
    private const string Baseline = "O:BAG:SYD:(A;;0x1201bf;;;WD)(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x1200a9;;;RC)S:(ML;;NW;;;LW)";
    private static byte[] Descriptor(string sddl) {
        var value = new RawSecurityDescriptor(sddl); var bytes = new byte[value.BinaryLength];
        value.GetBinaryForm(bytes, 0); return bytes;
    }
    private static void Require(bool value, string name) { if (!value) throw new InvalidOperationException("Control failed: " + name); }
    private static void Reject(Action action, string name) {
        try { action(); } catch (InvalidOperationException) { return; }
        throw new InvalidOperationException("Control failed to reject: " + name);
    }
    public static Dictionary<string, object> Run() {
        var baseline = Descriptor(Baseline);
        var plan = new ArgentumCiNullPlan(baseline);
        Require(plan.Added, "single missing ACE added");
        Require((ArgentumCiNullPlan.Mask & (0x20 | 0x40000 | 0x80000 | 0x10000)) == 0, "no execute/delete/permission-management access");
        ArgentumCiNullPlan.ValidateOnlyDaclDelta(plan.Before, plan.Prepared, true);
        var control = new ArgentumCiFakeNull(baseline);
        using (var scope = new ArgentumCiNullScope(control)) {
            scope.Apply(); Require(scope.Applied && control.Writes == 1, "apply/readback");
            scope.Restore(); Require(scope.Restored && control.Writes == 2, "restore/readback");
            Require(ArgentumCiNullPlan.Equal(baseline, control.State), "exact original descriptor");
            Require((string)scope.Receipt()["before_sha256"] == (string)scope.Receipt()["current_sha256"], "receipt current fingerprint");
        }
        Require(control.Closed, "owned handle closed");
        var ready = new ArgentumCiFakeNull(plan.Prepared);
        using (var scope = new ArgentumCiNullScope(ready)) { scope.Apply(); scope.Restore(); Require(!scope.Plan.Added && ready.Writes == 0, "existing access is a no-op"); }
        var failedBody = new ArgentumCiFakeNull(baseline);
        using (var scope = new ArgentumCiNullScope(failedBody)) {
            try { scope.Apply(); throw new Exception("Synthetic body failure"); }
            catch (Exception) { }
            finally { scope.Restore(); }
            Require(scope.Restored && ArgentumCiNullPlan.Equal(baseline, failedBody.State), "body exception cleanup");
        }
        var lost = new ArgentumCiFakeNull(baseline) { ThrowAfterFirstWrite = true };
        using (var scope = new ArgentumCiNullScope(lost)) {
            Reject(() => scope.Apply(), "write result uncertainty");
            scope.Restore(); Require(scope.Restored && lost.Writes == 2, "uncertain apply still restored");
        }
        var drift = new ArgentumCiFakeNull(baseline);
        using (var scope = new ArgentumCiNullScope(drift)) {
            drift.State = Descriptor(Baseline.Replace("0x1200a9", "0x120089"));
            Reject(() => scope.Apply(), "pre-apply drift"); Require(drift.Writes == 0, "no mutation after drift");
        }
        var concurrent = new ArgentumCiFakeNull(baseline);
        using (var scope = new ArgentumCiNullScope(concurrent)) {
            scope.Apply(); concurrent.State = Descriptor(Baseline.Replace("0x1200a9", "0x120089"));
            Reject(() => scope.Restore(), "concurrent change not overwritten");
            Require(!scope.Restored && concurrent.Writes == 1, "failed cleanup not reported as restored");
        }
        var cleanup = new ArgentumCiFakeNull(baseline) { FailRestore = true };
        using (var scope = new ArgentumCiNullScope(cleanup)) { scope.Apply(); Reject(() => scope.Restore(), "restore error"); Require(!scope.Restored, "restore failure remains explicit"); }
        Reject(() => new ArgentumCiNullScope(new ArgentumCiFakeNull(baseline) { Identity = @"\Device\Other" }), "wrong device");
        Reject(() => new ArgentumCiNullPlan(Descriptor("O:BAG:SY")), "null DACL");
        Reject(() => new ArgentumCiNullPlan(Descriptor(Baseline.Replace("S:", "(D;;GR;;;S-1-15-2-2)S:"))), "conflicting package ACE");
        Reject(() => ArgentumCiNullPlan.ValidateOnlyDaclDelta(plan.Before, Descriptor(Baseline.Replace(";;;LW", ";;;ME")), false), "integrity label mutation");
        Reject(() => ArgentumCiNullGuard.RequireProperties(null, null, "Windows", true), "ordinary local environment");
        Reject(() => ArgentumCiNullGuard.RequireProperties("true", "self-hosted", "Windows", true), "self-hosted machine");
        Reject(() => ArgentumCiNullGuard.RequireProperties("true", "github-hosted", "Linux", false), "wrong OS");
        var groups = new[] {"minimal tuple", "apply and exact readback", "exact restoration", "owned closure", "existing-access no-op", "body exception", "uncertain write result", "pre-apply drift", "concurrent drift", "restore failure", "wrong device", "null DACL", "conflicting ACE", "integrity change rejection", "environment guards"};
        return new Dictionary<string, object> { {"state", "passed"}, {"control_groups", groups}, {"controls", groups.Length}, {"boundary", "synthetic in-memory ACL device; no native NUL opened"}, {"local_device_permissions_changed", false} };
    }
}
