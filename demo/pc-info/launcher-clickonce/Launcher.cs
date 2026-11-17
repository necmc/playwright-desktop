using System;
using System.Configuration;
using System.Deployment.Application;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Text;
using System.Text.RegularExpressions;
using System.Web;
using System.Windows.Forms;

internal static class Launcher
{
    [STAThread]
    private static int Main(string[] args)
    {
        Uri operation = null;
        try
        {
            string address;
            if (ApplicationDeployment.IsNetworkDeployed)
            {
                var activation = ApplicationDeployment.CurrentDeployment.ActivationUri;
                if (activation == null) throw new InvalidOperationException("ClickOnce no proporcionó la dirección de activación.");
                address = HttpUtility.ParseQueryString(activation.Query)["operation"];
            }
            else
            {
                address = args.Length == 1 ? args[0] : null;
            }
            Uri candidate;
            if (!Uri.TryCreate(address, UriKind.Absolute, out candidate)
                || candidate.Scheme != "http"
                || !(candidate.Host == "127.0.0.1" || candidate.Host == "localhost")
                || candidate.UserInfo.Length != 0 || candidate.Query.Length != 0 || candidate.Fragment.Length != 0
                || !Regex.IsMatch(candidate.AbsolutePath, "^/api/operations/[A-Za-z0-9_-]{20,64}$"))
            {
                throw new InvalidOperationException("Abre esta aplicación desde Obtener información en la web local.");
            }
            operation = candidate;
            var directory = AppDomain.CurrentDomain.BaseDirectory;
            var javaHome = ConfigurationManager.AppSettings["JavaHome"];
            var java = Path.Combine(javaHome ?? "", "bin", "javaw.exe");
            var jar = Path.Combine(directory, "InformationForm.jar");
            if (!File.Exists(java)) throw new FileNotFoundException("No se encontró Java. Vuelve a compilar indicando -JavaHome con la carpeta de OpenJDK.");
            if (!File.Exists(jar)) throw new FileNotFoundException("La publicación no contiene InformationForm.jar.");
            var bridgeOptions = String.Equals(ConfigurationManager.AppSettings["EnableAccessBridge"], "true", StringComparison.OrdinalIgnoreCase)
                ? "-Djavax.accessibility.assistive_technologies=com.sun.java.accessibility.AccessBridge -Djavax.accessibility.screen_magnifier_present=true "
                : "";
            var start = new ProcessStartInfo
            {
                FileName = java,
                Arguments = bridgeOptions + "-jar \"" + jar + "\" \"" + operation.AbsoluteUri + "\"",
                WorkingDirectory = directory,
                UseShellExecute = false
            };
            using (var process = Process.Start(start))
            {
                if (process == null) throw new InvalidOperationException("No se pudo iniciar el formulario Java.");
                process.WaitForExit();
                if (process.ExitCode != 0) throw new InvalidOperationException("La aplicación Java terminó con un error.");
            }
            return 0;
        }
        catch (Exception error)
        {
            if (operation != null) ReportError(operation, error.Message);
            MessageBox.Show(error.Message, "No se pudo abrir la demo", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return 1;
        }
    }

    private static void ReportError(Uri operation, string message)
    {
        try
        {
            var request = (HttpWebRequest)WebRequest.Create(operation.AbsoluteUri + "/error");
            request.Method = "POST";
            request.ContentType = "application/x-www-form-urlencoded; charset=UTF-8";
            request.Timeout = 5000;
            var body = Encoding.UTF8.GetBytes("message=" + HttpUtility.UrlEncode(message));
            request.ContentLength = body.Length;
            using (var stream = request.GetRequestStream()) stream.Write(body, 0, body.Length);
            using (var response = request.GetResponse()) { }
        }
        catch (WebException)
        {
            // El mensaje local sigue disponible cuando el servidor no responde.
        }
    }
}
