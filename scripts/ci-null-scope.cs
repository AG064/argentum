// CI host preparation only. Never linked into Argentum or invoked at startup.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Security.Principal;
using System.Security.Cryptography;

public static class ArgentumCiNullGuard {
    public static void RequireProperties(string actions, string environment, string os, bool windows) {
        if (!windows || actions != "true" || environment != "github-hosted" || os != "Windows")
            throw new InvalidOperationException("NUL preparation is restricted to the approved disposable GitHub Windows job");
    }
    public static void RequireHostedWindows() {
        RequireProperties(Environment.GetEnvironmentVariable("GITHUB_ACTIONS"),
            Environment.GetEnvironmentVariable("RUNNER_ENVIRONMENT"),
            Environment.GetEnvironmentVariable("RUNNER_OS"), Environment.OSVersion.Platform == PlatformID.Win32NT);
        // These environment checks prevent accidental local use. They are not
        // authentication against code already running as the same OS user.
        if (!new WindowsPrincipal(WindowsIdentity.GetCurrent()).IsInRole(WindowsBuiltInRole.Administrator))
            throw new InvalidOperationException("CI token lacks existing administrative rights; no elevation attempted");
    }
}

public interface IArgentumCiNullDevice : IDisposable {
    string Identity { get; }
    byte[] Read();
    void WriteDacl(byte[] descriptor);
}

public sealed class ArgentumCiNullPlan {
    public const string DeviceName = @"\Device\Null";
    public const string Trustee = "S-1-15-2-2";
    public const int Mask = 0x0012019f; // FILE_GENERIC_READ | FILE_GENERIC_WRITE.
    public readonly byte[] Before;
    public readonly byte[] Prepared;
    public readonly bool Added;

    private static byte[] Encode(RawSecurityDescriptor descriptor) {
        var result = new byte[descriptor.BinaryLength];
        descriptor.GetBinaryForm(result, 0);
        return result;
    }
    public static byte[] Canonical(byte[] bytes) {
        if (bytes == null || bytes.Length == 0 || bytes.Length > 65536)
            throw new InvalidOperationException("Invalid bounded NUL descriptor");
        return Encode(new RawSecurityDescriptor(bytes, 0));
    }
    public static string Digest(byte[] bytes) {
        using (var hash = SHA256.Create())
            return BitConverter.ToString(hash.ComputeHash(Canonical(bytes))).Replace("-", "").ToLowerInvariant();
    }
    public static bool Equal(byte[] left, byte[] right) {
        return Digest(left) == Digest(right);
    }
    public ArgentumCiNullPlan(byte[] original) {
        Before = Canonical(original);
        var next = new RawSecurityDescriptor(Before, 0);
        if (next.Owner == null || next.Group == null || next.DiscretionaryAcl == null ||
            (next.ControlFlags & ControlFlags.DiscretionaryAclPresent) == 0)
            throw new InvalidOperationException("Refuse absent owner/group or null DACL");
        var sid = new SecurityIdentifier(Trustee);
        var found = false;
        foreach (GenericAce entry in next.DiscretionaryAcl) {
            var common = entry as CommonAce;
            if (common != null && common.SecurityIdentifier.Equals(sid)) {
                if (common.IsCallback || common.AceQualifier != AceQualifier.AccessAllowed ||
                    common.AceFlags != AceFlags.None || (common.AccessMask & Mask) != Mask)
                    throw new InvalidOperationException("Conflicting restricted-package NUL ACE; do not merge or remove it");
                found = true;
            }
        }
        if (!found) {
            // A separate exact tuple: do not coalesce other ACEs or reorder them.
            next.DiscretionaryAcl.InsertAce(next.DiscretionaryAcl.Count,
                new CommonAce(AceFlags.None, AceQualifier.AccessAllowed, Mask, sid, false, null));
        }
        Added = !found;
        Prepared = Encode(next);
        ValidateOnlyDaclDelta(Before, Prepared, Added);
    }
    public static void ValidateOnlyDaclDelta(byte[] before, byte[] after, bool added) {
        var left = new RawSecurityDescriptor(Canonical(before), 0);
        var right = new RawSecurityDescriptor(Canonical(after), 0);
        if (!left.Owner.Equals(right.Owner) || !left.Group.Equals(right.Group) || left.ControlFlags != right.ControlFlags)
            throw new InvalidOperationException("Owner/group/control flags changed");
        var l = left.SystemAcl; var r = right.SystemAcl;
        if ((l == null) != (r == null)) throw new InvalidOperationException("Integrity label changed");
        if (l != null) {
            var lb = new byte[l.BinaryLength]; l.GetBinaryForm(lb, 0);
            var rb = new byte[r.BinaryLength]; r.GetBinaryForm(rb, 0);
            if (!BytesEqual(lb, rb)) throw new InvalidOperationException("Integrity label changed");
        }
        if (right.DiscretionaryAcl.Count != left.DiscretionaryAcl.Count + (added ? 1 : 0))
            throw new InvalidOperationException("Unexpected ACL delta");
        for (int index = 0; index < left.DiscretionaryAcl.Count; index++) {
            var a = new byte[left.DiscretionaryAcl[index].BinaryLength]; left.DiscretionaryAcl[index].GetBinaryForm(a, 0);
            var b = new byte[right.DiscretionaryAcl[index].BinaryLength]; right.DiscretionaryAcl[index].GetBinaryForm(b, 0);
            if (!BytesEqual(a, b)) throw new InvalidOperationException("Existing ACE changed");
        }
        if (added) {
            var ace = right.DiscretionaryAcl[right.DiscretionaryAcl.Count - 1] as CommonAce;
            if (ace == null || ace.IsCallback || ace.AceFlags != AceFlags.None ||
                ace.AceQualifier != AceQualifier.AccessAllowed || ace.AccessMask != Mask ||
                ace.SecurityIdentifier.Value != Trustee)
                throw new InvalidOperationException("Added ACE differs from approved tuple");
        }
    }
    private static bool BytesEqual(byte[] a, byte[] b) {
        if (a.Length != b.Length) return false;
        for (int i = 0; i < a.Length; i++) if (a[i] != b[i]) return false;
        return true;
    }
}

public sealed class ArgentumCiNullScope : IDisposable {
    private readonly IArgentumCiNullDevice device;
    public readonly ArgentumCiNullPlan Plan;
    public bool WriteAttempted { get; private set; }
    public bool Applied { get; private set; }
    public bool Restored { get; private set; }
    public ArgentumCiNullScope(IArgentumCiNullDevice device) {
        if (device.Identity != ArgentumCiNullPlan.DeviceName)
            throw new InvalidOperationException("Not the exact NUL kernel device");
        this.device = device;
        Plan = new ArgentumCiNullPlan(device.Read());
    }
    public void Apply() {
        if (!ArgentumCiNullPlan.Equal(device.Read(), Plan.Before))
            throw new InvalidOperationException("NUL descriptor changed before apply");
        if (Plan.Added) {
            WriteAttempted = true; // Retain possible effects even if the API fails.
            device.WriteDacl(Plan.Prepared);
        }
        if (!ArgentumCiNullPlan.Equal(device.Read(), Plan.Prepared))
            throw new InvalidOperationException("NUL readback differs from exact prepared descriptor");
        Applied = true;
    }
    public void Restore() {
        var current = device.Read();
        if (ArgentumCiNullPlan.Equal(current, Plan.Before)) { Restored = true; return; }
        if (!WriteAttempted || !ArgentumCiNullPlan.Equal(current, Plan.Prepared))
            throw new InvalidOperationException("Unexpected descriptor drift; refusing to overwrite concurrent state");
        device.WriteDacl(Plan.Before);
        if (!ArgentumCiNullPlan.Equal(device.Read(), Plan.Before))
            throw new InvalidOperationException("Original NUL descriptor was not restored");
        Restored = true;
    }
    public Dictionary<string, object> Receipt() {
        string current = null;
        try { current = ArgentumCiNullPlan.Digest(device.Read()); } catch { /* Unknown is not restored evidence. */ }
        return new Dictionary<string, object> {
            {"schema_version", 1}, {"device", device.Identity}, {"trustee", ArgentumCiNullPlan.Trustee},
            {"mask", "0x0012019f"}, {"inheritance", "none"}, {"added", Plan.Added},
            {"write_attempted", WriteAttempted}, {"applied", Applied}, {"restored", Restored},
            {"before_sha256", ArgentumCiNullPlan.Digest(Plan.Before)},
            {"prepared_sha256", ArgentumCiNullPlan.Digest(Plan.Prepared)},
            {"current_sha256", current},
            {"owner_group_label_write_requested", false}, {"full_sacl_write_requested", false}
        };
    }
    public void Dispose() { device.Dispose(); }
}

public sealed class ArgentumCiNativeNull : IArgentumCiNullDevice {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] private static extern IntPtr CreateFileW(string name, uint access, uint share, IntPtr attrs, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)] private static extern bool CloseHandle(IntPtr handle);
    [DllImport("advapi32.dll", SetLastError=true)] private static extern bool GetKernelObjectSecurity(IntPtr handle, uint info, IntPtr data, uint length, out uint needed);
    [DllImport("advapi32.dll", SetLastError=true)] private static extern bool SetKernelObjectSecurity(IntPtr handle, uint info, IntPtr data);
    [DllImport("ntdll.dll")] private static extern int NtQueryObject(IntPtr handle, uint kind, IntPtr data, uint length, out uint needed);
    [StructLayout(LayoutKind.Sequential)] private struct UnicodeName { public ushort Length; public ushort MaximumLength; public IntPtr Buffer; }
    private IntPtr handle;
    public string Identity { get; private set; }
    public ArgentumCiNativeNull() {
        ArgentumCiNullGuard.RequireHostedWindows(); // Before any writable handle.
        // No WRITE_OWNER, security privilege enablement or arbitrary device path.
        handle = CreateFileW(@"\\.\NUL", 0x80060000, 7, IntPtr.Zero, 3, 0x80, IntPtr.Zero);
        if (handle == new IntPtr(-1)) { handle = IntPtr.Zero; throw new Win32Exception(Marshal.GetLastWin32Error()); }
        try {
            uint needed;
            NtQueryObject(handle, 1, IntPtr.Zero, 0, out needed);
            if (needed < Marshal.SizeOf(typeof(UnicodeName)) || needed > 65536)
                throw new InvalidOperationException("Invalid bounded NUL object-name buffer");
            var buffer = Marshal.AllocHGlobal((int)needed);
            try {
                var capacity = needed;
                if (NtQueryObject(handle, 1, buffer, capacity, out needed) < 0)
                    throw new InvalidOperationException("NUL kernel identity could not be queried");
                var name = (UnicodeName)Marshal.PtrToStructure(buffer, typeof(UnicodeName));
                var offset = name.Buffer.ToInt64() - buffer.ToInt64();
                if ((name.Length & 1) != 0 || offset < 0 || offset > capacity || name.Length > capacity - offset)
                    throw new InvalidOperationException("Invalid NUL kernel name");
                Identity = Marshal.PtrToStringUni(name.Buffer, name.Length / 2);
                if (Identity != ArgentumCiNullPlan.DeviceName) throw new InvalidOperationException("Device alias did not resolve to NUL");
            } finally { Marshal.FreeHGlobal(buffer); }
        } catch { Dispose(); throw; }
    }
    public byte[] Read() {
        uint needed;
        // Owner/group/DACL and mandatory label only. Full SACL is not requested.
        GetKernelObjectSecurity(handle, 0x17, IntPtr.Zero, 0, out needed);
        if (needed == 0 || needed > 65536) throw new InvalidOperationException("Invalid bounded NUL security buffer");
        var buffer = Marshal.AllocHGlobal((int)needed);
        try {
            var capacity = needed;
            if (!GetKernelObjectSecurity(handle, 0x17, buffer, capacity, out needed)) throw new Win32Exception(Marshal.GetLastWin32Error());
            if (needed > capacity) throw new InvalidOperationException("NUL descriptor grew during query");
            var bytes = new byte[needed]; Marshal.Copy(buffer, bytes, 0, bytes.Length);
            return ArgentumCiNullPlan.Canonical(bytes);
        } finally { Marshal.FreeHGlobal(buffer); }
    }
    public void WriteDacl(byte[] descriptor) {
        var bytes = ArgentumCiNullPlan.Canonical(descriptor);
        var buffer = Marshal.AllocHGlobal(bytes.Length);
        try {
            Marshal.Copy(bytes, 0, buffer, bytes.Length);
            if (!SetKernelObjectSecurity(handle, 4, buffer)) throw new Win32Exception(Marshal.GetLastWin32Error());
        } finally { Marshal.FreeHGlobal(buffer); }
    }
    public void Dispose() {
        if (handle != IntPtr.Zero) {
            var owned = handle;
            if (!CloseHandle(owned)) throw new Win32Exception(Marshal.GetLastWin32Error());
            handle = IntPtr.Zero;
        }
    }
}
