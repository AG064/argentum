# Read-only device-security metadata for the inherited-stdio comparison.
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ArgentumNullDescriptor {
 [DllImport("kernel32.dll", CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateFileW(string p,uint a,uint s,IntPtr sa,uint d,uint f,IntPtr t);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr p);
 [DllImport("advapi32.dll")] static extern uint GetSecurityInfo(IntPtr h,int type,uint info,out IntPtr owner,out IntPtr group,out IntPtr dacl,out IntPtr sacl,out IntPtr sd);
 [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ConvertSecurityDescriptorToStringSecurityDescriptorW(IntPtr sd,uint revision,uint info,out IntPtr text,out uint len);
 public static string Read() {
  var h=CreateFileW("NUL",0x20000,3,IntPtr.Zero,3,0,IntPtr.Zero);
  if(h==new IntPtr(-1)) return "open_error="+Marshal.GetLastWin32Error();
  try {IntPtr owner,group,dacl,sacl,sd;var code=GetSecurityInfo(h,1,0x17,out owner,out group,out dacl,out sacl,out sd);
   if(code!=0)return "query_error="+code;
   try {IntPtr text;uint len;if(!ConvertSecurityDescriptorToStringSecurityDescriptorW(sd,1,0x17,out text,out len))return "format_error="+Marshal.GetLastWin32Error();try{return Marshal.PtrToStringUni(text);}finally{LocalFree(text);}}
   finally{LocalFree(sd);}
  }finally{CloseHandle(h);}
 }
}
'@
[ArgentumNullDescriptor]::Read() -replace 'S-1-5-21-(?:\d+-)*\d+','<user-sid>'
