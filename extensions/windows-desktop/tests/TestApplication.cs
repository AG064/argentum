using System;
using System.IO;
using System.Linq;
using System.Threading;
using System.Windows.Forms;

internal static class TestApplication
{
    [STAThread] private static void Main(string[] args)
    {
        Application.EnableVisualStyles();
        var form=new Form { Text="Owned synthetic editor", Width=700, Height=500 };
        var editor=new TextBox { Name="Document", Multiline=true, ScrollBars=ScrollBars.Vertical, Dock=DockStyle.Fill, Text=File.ReadAllText(args[args.Length-1]) };
        var buttons=new FlowLayoutPanel { Dock=DockStyle.Top, Height=36 };
        var save=new Button { Text="Save As" };
        var replace=new Button { Text="Replace editor" };
        var hang=new Button { Text="Hang provider" };
        var unexpected=new Button { Text="Unexpected dialog" };
        buttons.Controls.Add(save); buttons.Controls.Add(replace); buttons.Controls.Add(hang); buttons.Controls.Add(unexpected);
        form.Controls.Add(editor); form.Controls.Add(buttons);
        if(args.Contains("--password-probe")) {
            var password=new TextBox { AccessibleName="Synthetic password control",UseSystemPasswordChar=true,Text="NOT_A_REAL_SECRET",Dock=DockStyle.Bottom };
            form.Controls.Add(password);
        }
        System.Windows.Forms.Timer decoyTimer=null;
        if(args.Contains("--same-process-decoy")) {
            decoyTimer=new System.Windows.Forms.Timer { Interval=1200 };
            decoyTimer.Tick+=(s,e)=> { decoyTimer.Stop(); new Form { Text=form.Text,Width=320,Height=160 }.Show(); };
            form.Shown+=(s,e)=>decoyTimer.Start();
        }
        save.Click+=(s,e)=> { using(var dialog=new SaveFileDialog { InitialDirectory=Path.GetDirectoryName(args[args.Length-1]), Filter="Text files|*.txt", Title="Save As" }) { if(dialog.ShowDialog(form)==DialogResult.OK) File.WriteAllText(dialog.FileName,editor.Text); } };
        if(args.Contains("--native-menu-controls")) {
            var selectAll=new MenuItem("Select &All",(s,e)=>editor.SelectAll());
            var saveAs=new MenuItem("Save &As...",(s,e)=>save.PerformClick());
            var open=new MenuItem("&Open...",(s,e)=> { using(var dialog=new OpenFileDialog { InitialDirectory=Path.GetDirectoryName(args[args.Length-1]),Title="Open" }) { if(dialog.ShowDialog(form)==DialogResult.OK) editor.Text=File.ReadAllText(dialog.FileName); } });
            var find=new MenuItem("&Find...",(s,e)=> {
                using(var dialog=new Form { Text="Find",Width=360,Height=150,Owner=form }) {
                    var field=new TextBox { Name="Find text",Dock=DockStyle.Top };
                    var next=new Button { Text="Find Next",Dock=DockStyle.Bottom };
                    var close=new Button { Text="Close",Dock=DockStyle.Bottom };
                    next.Click+=(ns,ne)=> { int index=editor.Text.IndexOf(field.Text,StringComparison.Ordinal); if(index>=0) editor.Select(index,field.Text.Length); };
                    close.Click+=(cs,ce)=>dialog.Close();
                    dialog.Controls.Add(field);dialog.Controls.Add(close);dialog.Controls.Add(next);
                    dialog.ShowDialog(form);
                }
            });
            form.Menu=new MainMenu(new[] { new MenuItem("&File",new[]{open,saveAs}),new MenuItem("&Edit",new[]{selectAll}),new MenuItem("&Search",new[]{find,new MenuItem("Find &Next",(s,e)=>editor.Select(0,1))}) });
            form.Shown+=(s,e)=>editor.Focus();
        }
        replace.Click+=(s,e)=> { form.Controls.Remove(editor); editor.Dispose(); editor=new TextBox { Name="Document",Multiline=true,Dock=DockStyle.Fill,Text="Replacement control" }; form.Controls.Add(editor); editor.BringToFront(); };
        hang.Click+=(s,e)=>Thread.Sleep(15000);
        unexpected.Click+=(s,e)=>MessageBox.Show(form,"Synthetic boundary control","Unexpected owned dialog");
        Application.Run(form);
    }
}
