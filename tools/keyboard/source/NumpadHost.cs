using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Windows.Forms;
using System.IO;
using System.Collections.Generic;
using Keyboard2XinputLib;

// Physical keypad scan codes keep the separate keypad distinct from the number
// row and navigation keys, including when Num Lock is off or Shift is held.
static class NumpadHost
{
    delegate IntPtr HookProc(int code, IntPtr message, IntPtr data);
    [StructLayout(LayoutKind.Sequential)]
    struct KeyboardData
    {
        public uint VirtualKey, ScanCode, Flags, Time;
        public UIntPtr ExtraInfo;
    }
    [DllImport("user32.dll", SetLastError = true)]
    static extern IntPtr SetWindowsHookEx(int type, HookProc callback, IntPtr module, uint thread);
    [DllImport("user32.dll")]
    static extern bool UnhookWindowsHookEx(IntPtr hook);
    [DllImport("user32.dll")]
    static extern IntPtr CallNextHookEx(IntPtr hook, int code, IntPtr message, IntPtr data);
    [DllImport("kernel32.dll", CharSet = CharSet.Auto)]
    static extern IntPtr GetModuleHandle(string module);
    static IntPtr hook;
    static HookProc callback = OnKey;
    static Keyboard2Xinput mapper;
    static System.Windows.Forms.Timer mappingTimer;
    static DateTime mappingStamp;
    static string mappingPath;
    static bool keypadLayer;
    static Dictionary<Keys, string> heldChords = new Dictionary<Keys, string>();

    static void RefreshContext()
    {
        int context = 2;
        string path = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "skate-context.txt");
        try
        {
            if (DateTime.UtcNow - File.GetLastWriteTimeUtc(path) < TimeSpan.FromSeconds(2))
            {
                string[] fields = File.ReadAllText(path).Trim().Split(':');
                int pid = Int32.Parse(fields[0]);
                using (Process game = Process.GetProcessById(pid))
                    if (game.ProcessName == "iw4l") context = Int32.Parse(fields[1]);
            }
        }
        catch { /* Missing or stale game state leaves gameplay mapping off. */ }
        mapper.SetSkateContext(context);
        if (context != 1) { keypadLayer = false; }
    }

    public static Keys NormalizeKey(uint virtualKey, uint scanCode, uint flags)
    {
        // Extended navigation keys are the main arrow/Home/etc. keys; pass them through.
        if ((flags & 1) == 0)
        {
            switch (scanCode)
            {
                case 0x47: return Keys.NumPad7;
                case 0x48: return Keys.NumPad8;
                case 0x49: return Keys.NumPad9;
                case 0x4B: return Keys.NumPad4;
                case 0x4C: return Keys.NumPad5;
                case 0x4D: return Keys.NumPad6;
                case 0x4F: return Keys.NumPad1;
                case 0x50: return Keys.NumPad2;
                case 0x51: return Keys.NumPad3;
                case 0x52: return Keys.NumPad0;
                case 0x53: return Keys.Decimal;
            }
        }
        return (Keys)virtualKey;
    }

    static IntPtr OnKey(int code, IntPtr message, IntPtr data)
    {
        if (code >= 0)
        {
            int kind = message.ToInt32();
            if (kind == 0x100 || kind == 0x101 || kind == 0x104 || kind == 0x105)
            {
                KeyboardData key = (KeyboardData)Marshal.PtrToStructure(data, typeof(KeyboardData));
                Keys normalized = NormalizeKey(key.VirtualKey, key.ScanCode, key.Flags);
                bool down = kind == 0x100 || kind == 0x104;
                // Only the separate keypad Enter is the extra-controls layer.
                // Ordinary Enter and every PC modifier remain untouched.
                if (key.VirtualKey == 0x0D && (key.Flags & 1) != 0 && mapper.CanUseSkateLayer())
                {
                    keypadLayer = down;
                    return new IntPtr(1);
                }
                string chord;
                if (!heldChords.TryGetValue(normalized, out chord))
                {
                    bool padKey = (normalized >= Keys.NumPad0 && normalized <= Keys.NumPad9)
                        || normalized == Keys.Decimal || normalized == Keys.Divide
                        || normalized == Keys.Multiply || normalized == Keys.Subtract || normalized == Keys.Add;
                    chord = (padKey && keypadLayer ? "NumEnter+" : "") + normalized.ToString();
                    if (down) heldChords[normalized] = chord;
                }
                if (!down) heldChords.Remove(normalized);
                int result = mapper.chordEvent(kind, chord);
                if (result < 0) { Application.Exit(); return new IntPtr(1); }
                if (result > 0) return new IntPtr(1);
            }
        }
        return CallNextHookEx(hook, code, message, data);
    }

    [STAThread]
    static int Main()
    {
        Application.EnableVisualStyles();
        try
        {
            mapper = new Keyboard2Xinput("mapping.ini");
            mappingPath = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "mapping.ini");
            mappingStamp = File.GetLastWriteTimeUtc(mappingPath);
            mappingTimer = new System.Windows.Forms.Timer { Interval = 200 };
            mappingTimer.Tick += delegate {
                RefreshContext();
                DateTime stamp = File.GetLastWriteTimeUtc(mappingPath);
                if (stamp == mappingStamp) return;
                try { mapper.ReloadMapping(mappingPath); heldChords.Clear(); mappingStamp = stamp; }
                catch { /* Keep the last valid controls if a file write is incomplete. */ }
            };
            mappingTimer.Start();
            hook = SetWindowsHookEx(13, callback, GetModuleHandle(null), 0);
            if (hook == IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
            using (NotifyIcon tray = new NotifyIcon())
            using (ContextMenuStrip menu = new ContextMenuStrip())
            {
                tray.Icon = System.Drawing.SystemIcons.Application;
                tray.Text = "MW2 Skate keyboard controller";
                menu.Items.Add("Enable numpad controller", null, delegate { mapper.Enable(); });
                menu.Items.Add("Disable numpad controller", null, delegate { mapper.Disable(); });
                menu.Items.Add("Exit", null, delegate { Application.Exit(); });
                tray.ContextMenuStrip = menu;
                tray.Visible = true;
                Application.Run();
                tray.Visible = false;
            }
            return 0;
        }
        catch (Exception error)
        {
            MessageBox.Show(error.Message, "MW2 Skate keyboard controller", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return 1;
        }
        finally
        {
            if (hook != IntPtr.Zero) UnhookWindowsHookEx(hook);
            if (mappingTimer != null) { mappingTimer.Stop(); mappingTimer.Dispose(); }
            if (mapper != null) mapper.Close();
        }
    }
}
