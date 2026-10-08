using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
using System.Windows.Forms;
using System.Runtime.InteropServices;

static class KeyboardLaunch
{
    [DllImport("xinput1_4.dll")]
    static extern uint XInputGetState(uint index, IntPtr state);

    static bool HasPad()
    {
        IntPtr state = Marshal.AllocHGlobal(32);
        try
        {
            for (uint i = 0; i < 4; i++)
                if (XInputGetState(i, state) == 0) return true;
            return false;
        }
        finally { Marshal.FreeHGlobal(state); }
    }

    [STAThread]
    static void Main(string[] args)
    {
        string root = AppDomain.CurrentDomain.BaseDirectory;
        string keyboard = Path.Combine(root, "keyboard");
        Process mapper = null;
        bool ownsMapper = false;
        bool ownsMutex;
        using (Mutex mutex = new Mutex(true, "MW2SkateKeyboardLauncher", out ownsMutex))
        {
            if (!ownsMutex) return;
            try
            {
                foreach (Process p in Process.GetProcessesByName("Keyboard2XinputGui"))
                {
                    try
                    {
                        if (String.Equals(p.MainModule.FileName,
                            Path.Combine(keyboard, "Keyboard2XinputGui.exe"),
                            StringComparison.OrdinalIgnoreCase)) { mapper = p; break; }
                    }
                    catch { }
                }
                if (mapper == null)
                {
                    mapper = Process.Start(new ProcessStartInfo(
                        Path.Combine(keyboard, "Keyboard2XinputGui.exe"))
                        { WorkingDirectory = keyboard, UseShellExecute = false });
                    ownsMapper = true;
                }
                bool ready = false;
                for (int i = 0; i < 40; i++)
                {
                    if (mapper.HasExited) break;
                    Thread.Sleep(250);
                    if (HasPad()) { ready = true; break; }
                }
                if (!ready)
                {
                    MessageBox.Show("The keyboard controller could not connect. Get the signed installer from " +
                        "https://github.com/nefarius/ViGEmBus/releases/tag/v1.22.0 to repair ViGEmBus, " +
                        "then restart Windows if requested and try this shortcut again.",
                        "MW2 Skate Keyboard", MessageBoxButtons.OK, MessageBoxIcon.Information);
                    return;
                }
                Process game = null;
                foreach (Process p in Process.GetProcessesByName("iw4l"))
                {
                    try
                    {
                        if (String.Equals(p.MainModule.FileName, Path.Combine(root, "iw4l.exe"),
                            StringComparison.OrdinalIgnoreCase)) { game = p; break; }
                    }
                    catch { }
                }
                if (game == null)
                    game = Process.Start(new ProcessStartInfo(Path.Combine(root, "iw4l.exe"), "map mp_rust --cmds \"spawn\"")
                        { WorkingDirectory = root, UseShellExecute = false });
                if (game != null) game.WaitForExit();
            }
            catch (Exception e)
            {
                MessageBox.Show(e.Message, "MW2 Skate Keyboard", MessageBoxButtons.OK,
                    MessageBoxIcon.Error);
            }
            finally
            {
                if (ownsMapper && mapper != null)
                {
                    try { if (!mapper.HasExited) mapper.Kill(); } catch { }
                    mapper.Dispose();
                }
                mutex.ReleaseMutex();
            }
        }
    }
}
