using System;
using System.Drawing;
using System.IO;
using System.Windows.Forms;

internal static class NativeControlWindow
{
    [STAThread]
    private static void Main(string[] args)
    {
        Application.EnableVisualStyles();
        using (var window = new Form { Text = args[0], ClientSize = new Size(360, 255) })
        {
            string results = args[1];
            AddButton(window, "Mostrar opciones", 10, 10, delegate
            {
                var dialog = new Form { Text = args[0] + " - Opciones", ClientSize = new Size(190, 70) };
                AddButton(dialog, "Aceptar", 10, 10, delegate
                {
                    File.AppendAllText(results, "dialog\n");
                    dialog.Close();
                });
                dialog.Show(window);
            });

            AddButton(window, "Aplicar", 10, 50, delegate { File.AppendAllText(results, "outside\n"); });
            var notice = new GroupBox
            {
                Text = "Opciones integradas",
                AccessibleName = "Opciones integradas",
                Location = new Point(10, 135),
                Size = new Size(260, 80),
                Visible = false
            };
            AddButton(notice, "Aplicar", 10, 24, delegate { File.AppendAllText(results, "inside\n"); });
            window.Controls.Add(notice);
            AddButton(window, "Mostrar aviso", 10, 90, delegate
            {
                notice.Visible = true;
                File.AppendAllText(results, "notice\n");
            });

            window.Shown += delegate { File.AppendAllText(results, "ready\n"); };
            Application.Run(window);
        }
    }

    private static void AddButton(Control parent, string name, int x, int y, EventHandler click)
    {
        var button = new Button
        {
            Text = name, AccessibleName = name, Location = new Point(x, y), Size = new Size(160, 32)
        };
        button.Click += click;
        parent.Controls.Add(button);
    }
}
