using System;
using System.Threading;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;

using System.Windows.Forms;
using IniParser;
using IniParser.Model;
using Nefarius.ViGEm.Client;
using Nefarius.ViGEm.Client.Targets;
using Nefarius.ViGEm.Client.Targets.Xbox360;
using Nefarius.ViGEm.Client.Exceptions;
using Keyboard2XinputLib.Exceptions;

namespace Keyboard2XinputLib
{
    public class Keyboard2Xinput
    {
        private static readonly log4net.ILog log = log4net.LogManager.GetLogger(System.Reflection.MethodBase.GetCurrentMethod().DeclaringType);
        public const int WM_KEYDOWN = 0x0100;
        public const int WM_KEYUP = 0x0101;
        public const int WM_SYSKEYDOWN = 0x0104;
        private Dictionary<string, Xbox360Buttons> buttonsDict = new Dictionary<string, Xbox360Buttons>();
        private Dictionary<string, KeyValuePair<Xbox360Axes, short>> axesDict = new Dictionary<string, KeyValuePair<Xbox360Axes, short>>();


        private ViGEmClient client;
        private List<Xbox360Controller> controllers;
        private List<Xbox360Report> reports;
        private List<ISet<Xbox360Buttons>> pressedButtons;
        private Config config;
        private Boolean enabled = true;
        private List<StateListener> listeners;
        private System.Windows.Forms.Timer focusTimer;
        private bool hadGameFocus;
        private bool numLockHeld;
        private System.Windows.Forms.Timer skatePulseTimer;
        private System.Windows.Forms.Timer ollieTimer;
        private bool ollieHeld;
        private int skateContext = 2;

        public void SetSkateContext(int context)
        {
            if (skateContext == 1 && context != 1) ResetReports();
            skateContext = context;
        }
        public bool CanUseSkateLayer() { return IsMashupFocused() && skateContext == 1; }

        private int HandleOllie(int eventType)
        {
            if (eventType == WM_KEYDOWN || eventType == WM_SYSKEYDOWN)
            {
                if (!ollieHeld)
                {
                    ollieHeld = true;
                    ollieTimer.Stop();
                    reports[0].SetAxis(Xbox360Axes.RightThumbX, 0);
                    reports[0].SetAxis(Xbox360Axes.RightThumbY, -30000);
                    controllers[0].SendReport(reports[0]);
                }
            }
            else if (ollieHeld)
            {
                ollieHeld = false;
                reports[0].SetAxis(Xbox360Axes.RightThumbY, 30000);
                controllers[0].SendReport(reports[0]);
                ollieTimer.Start();
            }
            return 1;
        }

        [DllImport("user32.dll")]
        private static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

        private static bool IsGameFocused()
        {
            try
            {
                uint processId;
                GetWindowThreadProcessId(GetForegroundWindow(), out processId);
                using (Process process = Process.GetProcessById((int)processId))
                    return String.Equals(process.ProcessName, "iw4l", StringComparison.OrdinalIgnoreCase)
                        || String.Equals(process.ProcessName, "xenia_canary", StringComparison.OrdinalIgnoreCase);
            }
            catch { return false; }
        }

        private static bool IsMashupFocused()
        {
            try
            {
                uint processId;
                GetWindowThreadProcessId(GetForegroundWindow(), out processId);
                using (Process process = Process.GetProcessById((int)processId))
                    return String.Equals(process.ProcessName, "iw4l", StringComparison.OrdinalIgnoreCase);
            }
            catch { return false; }
        }

        private void ReleaseSkateToggle()
        {
            skatePulseTimer.Stop();
            for (int i = 0; i < controllers.Count; i++)
            {
                reports[i].SetButtonState(Xbox360Buttons.LeftThumb, false);
                reports[i].SetButtonState(Xbox360Buttons.RightThumb, false);
                controllers[i].SendReport(reports[i]);
            }
        }

        private int HandleSkateToggle(int eventType)
        {
            if (eventType == WM_KEYDOWN || eventType == WM_SYSKEYDOWN)
            {
                if (!numLockHeld)
                {
                    numLockHeld = true;
                    Enable();
                    reports[0].SetButtonState(Xbox360Buttons.LeftThumb, true);
                    reports[0].SetButtonState(Xbox360Buttons.RightThumb, true);
                    controllers[0].SendReport(reports[0]);
                    skatePulseTimer.Stop();
                    skatePulseTimer.Start();
                }
            }
            else numLockHeld = false;
            return 1;
        }

        private bool RefreshFocus()
        {
            bool focused = IsGameFocused();
            if (hadGameFocus && !focused) ResetReports();
            hadGameFocus = focused;
            return focused;
        }

        private void ResetReports()
        {
            if (skatePulseTimer != null) skatePulseTimer.Stop();
            if (ollieTimer != null) ollieTimer.Stop();
            ollieHeld = false;
            numLockHeld = false;
            for (int i = 0; i < controllers.Count; i++)
            {
                reports[i] = new Xbox360Report();
                foreach (Xbox360Buttons button in buttonsDict.Values)
                    reports[i].SetButtonState(button, false);
                reports[i].SetAxis(Xbox360Axes.LeftThumbX, 0);
                reports[i].SetAxis(Xbox360Axes.LeftThumbY, 0);
                reports[i].SetAxis(Xbox360Axes.RightThumbX, 0);
                reports[i].SetAxis(Xbox360Axes.RightThumbY, 0);
                reports[i].SetAxis(Xbox360Axes.LeftTrigger, 0);
                reports[i].SetAxis(Xbox360Axes.RightTrigger, 0);
                pressedButtons[i].Clear();
                controllers[i].SendReport(reports[i]);
            }
        }

        public Keyboard2Xinput(String mappingFile)
        {
            config = new Config(mappingFile);

            InitializeAxesDict();
            InitializeButtonsDict();
            log.Debug("initialize dicts done.");

            // start enabled?
            String startEnabledStr = config.getCurrentMapping()["startup"]["enabled"];
            // only start disabled if explicitly configured as such
            if ((startEnabledStr != null) && ("false".Equals(startEnabledStr.ToLower())))
            {
                enabled = false;
            }

            // try to init ViGEm
            try
            {
                client = new ViGEmClient();
            }
            catch (VigemBusNotFoundException e)
            {
                throw new ViGEmBusNotFoundException("ViGEm bus not found, please make sure ViGEm is correctly installed.", e);
            }
            // create pads
            controllers = new List<Xbox360Controller>(config.PadCount);
            reports = new List<Xbox360Report>(config.PadCount);
            for (int i = 1; i <= config.PadCount; i++)
            {
                Xbox360Controller controller = new Xbox360Controller(client);
                controllers.Add(controller);
                controller.FeedbackReceived +=
                    (sender, eventArgs) => Console.WriteLine(
                        String.Format("LM: {0}, ", eventArgs.LargeMotor) +
                        String.Format("SM: {0}, ", eventArgs.SmallMotor) +
                        String.Format("LED: {0}", eventArgs.LedNumber));

                controller.Connect();
                reports.Add(new Xbox360Report());
                Thread.Sleep(1000);
            }
            // the pressed buttons (to avoid sending reports if the pressed buttons haven't changed)
            pressedButtons = new List<ISet<Xbox360Buttons>>(config.PadCount);
            for (int i = 0; i < config.PadCount; i++)
            {
                pressedButtons.Add(new HashSet<Xbox360Buttons>());
            }
            listeners = new List<StateListener>();
            skatePulseTimer = new System.Windows.Forms.Timer { Interval = 150 };
            skatePulseTimer.Tick += delegate { ReleaseSkateToggle(); };
            ollieTimer = new System.Windows.Forms.Timer { Interval = 120 };
            ollieTimer.Tick += delegate {
                ollieTimer.Stop();
                reports[0].SetAxis(Xbox360Axes.RightThumbY, 0);
                controllers[0].SendReport(reports[0]);
            };
            ResetReports();
            focusTimer = new System.Windows.Forms.Timer { Interval = 100 };
            focusTimer.Tick += delegate { RefreshFocus(); };
            focusTimer.Start();
        }

        public void AddListener(StateListener listener)
        {
            listeners.Add(listener);
        }

        private void NotifyListeners(Boolean enabled)
        {
            listeners.ForEach(delegate (StateListener listener)
            {
                listener.NotifyEnabled(enabled);
            });
        }


        /// <summary>
        /// handles key events
        /// </summary>
        /// <param name="eventType"></param>
        /// <param name="vkCode"></param>
        /// <returns>1 if the event has been handled, 0 if the key was not mapped, and -1 if the exit key has been pressed</returns>
        public int keyEvent(int eventType, Keys vkCode)
        { return chordEvent(eventType, vkCode.ToString()); }

        public int chordEvent(int eventType, string chord)
        {
            if (!RefreshFocus()) return 0;
            if ("skateToggle".Equals(config.getCurrentMapping()["config"][chord]))
                return IsMashupFocused() ? (skateContext == 2 ? 0 : HandleSkateToggle(eventType)) : 0;
            if (IsMashupFocused() && skateContext != 1) return 0;
            return HandleMappedKey(eventType, chord);
        }

        private int HandleGameKey(int eventType, Keys vkCode)
        { return HandleMappedKey(eventType, vkCode.ToString()); }

        private int HandleMappedKey(int eventType, string chord)
        {
            int handled = 0;

            if (enabled)
            {

                for (int i = 0; i < config.PadCount; i++)
                {
                    int padNumberForDisplay = i + 1;
                    string sectionName = "pad" + (padNumberForDisplay);
                    // is the key pressed mapped to a button?
                    string mappedButton = config.getCurrentMapping()[sectionName][chord];
                    if (mappedButton != null)
                    {
                        if (mappedButton == "OLLIE") return HandleOllie(eventType);
                        if (buttonsDict.ContainsKey(mappedButton))
                        {
                            if ((eventType == WM_KEYDOWN) || (eventType == WM_SYSKEYDOWN))
                            {
                                // if we already notified the virtual pad, don't do it again
                                Xbox360Buttons pressedButton = buttonsDict[mappedButton];
                                if (pressedButtons[i].Contains(pressedButton))
                                {
                                    handled = 1;
                                    break;
                                }
                                // store the state of the button
                                pressedButtons[i].Add(pressedButton);
                                reports[i].SetButtonState(pressedButton, true);
                                if (log.IsDebugEnabled)
                                {
                                    log.Debug(String.Format("pad{0} {1} down", padNumberForDisplay, mappedButton));
                                }
                            }
                            else
                            {
                                Xbox360Buttons pressedButton = buttonsDict[mappedButton];
                                reports[i].SetButtonState(pressedButton, false);
                                // remove the button state from our own set
                                pressedButtons[i].Remove(pressedButton);
                                if (log.IsDebugEnabled)
                                {
                                    log.Debug(String.Format("pad{0} {1} up", padNumberForDisplay, mappedButton));
                                }
                            }
                            controllers[i].SendReport(reports[i]);
                            handled = 1;
                            break;
                        }
                        else if (axesDict.ContainsKey(mappedButton))
                        {
                            KeyValuePair<Xbox360Axes, short> axisValuePair = axesDict[mappedButton];
                            if ((eventType == WM_KEYDOWN) || (eventType == WM_SYSKEYDOWN))
                            {
                                reports[i].SetAxis(axisValuePair.Key, axisValuePair.Value);
                                if (log.IsDebugEnabled)
                                {
                                    log.Debug(String.Format("pad{0} {1} down", padNumberForDisplay, mappedButton));
                                }
                            }
                            else
                            {
                                reports[i].SetAxis(axisValuePair.Key, 0x0);
                                if (log.IsDebugEnabled)
                                {
                                    log.Debug(String.Format("pad{0} {1} up", padNumberForDisplay, mappedButton));
                                }
                            }
                            controllers[i].SendReport(reports[i]);
                            handled = 1;
                            break;
                        }

                    }
                }
            }
            if (handled == 0)
            {
                // handle the enable toggle key even if disabled (otherwise there's not much point to it...)
                string enableButton = config.getCurrentMapping()["config"][chord];
                if ("enableToggle".Equals(enableButton))
                {
                    if ((eventType == WM_KEYDOWN) || (eventType == WM_SYSKEYDOWN))
                    {
                        ToggleEnabled();
                        if (log.IsInfoEnabled)
                        {
                            log.Info(String.Format("enableToggle down; enabled={0}", enabled));
                        }
                    }
                    handled = 1;
                }
                else if ("enable".Equals(enableButton))
                {
                    if ((eventType == WM_KEYDOWN) || (eventType == WM_SYSKEYDOWN))
                    {
                        Enable();
                        if (log.IsInfoEnabled)
                        {
                            log.Info(String.Format("enable down; enabled={0}", enabled));
                        }
                    }
                    handled = 1;
                }
                else if ("disable".Equals(enableButton))
                {
                    if ((eventType == WM_KEYDOWN) || (eventType == WM_SYSKEYDOWN))
                    {
                        Disable();
                        if (log.IsInfoEnabled)
                        {
                            log.Info(String.Format("disable down; enabled={0}", enabled));
                        }
                    }
                    handled = 1;
                }
                // key that exits the software
                string configButton = config.getCurrentMapping()["config"][chord];
                if ("exit".Equals(configButton))
                {
                    handled = -1;
                }
                else if ((configButton != null) && configButton.StartsWith("config"))
                {
                    if ((eventType == WM_KEYDOWN) || (eventType == WM_SYSKEYDOWN))
                    {
                        int index = Int32.Parse(configButton.Substring(configButton.Length - 1));
                        if (log.IsInfoEnabled)
                        {
                            log.Info(String.Format("Switching to mapping {0}", index));
                        }
                        config.CurrentMappingIndex = index;
                    }
                    handled = 1;
                }
            }

            if (handled == 0 && enabled && log.IsWarnEnabled)
            {
                log.Warn(String.Format("unmapped button {0}", chord));
            }

            return handled;
        }
        private void InitializeAxesDict()
        {
            // a bit weird: left& right thumb axes max values are 0x7530 (max short value), but left & right triggers max value are 0xFF
            short triggerValue = 0xFF;
            short posAxisValue = 0x7530;
            short negAxisValue = -0x7530;
            axesDict.Add("LT", new KeyValuePair<Xbox360Axes, short>(Xbox360Axes.LeftTrigger, triggerValue));
            axesDict.Add("RT", new KeyValuePair<Xbox360Axes, short>(Xbox360Axes.RightTrigger, triggerValue));
            axesDict.Add("LLEFT", new KeyValuePair<Xbox360Axes, short>(Xbox360Axes.LeftThumbX, negAxisValue));
            axesDict.Add("LRIGHT", new KeyValuePair<Xbox360Axes, short>(Xbox360Axes.LeftThumbX, posAxisValue));
            axesDict.Add("LUP", new KeyValuePair<Xbox360Axes, short>(Xbox360Axes.LeftThumbY, posAxisValue));
            axesDict.Add("LDOWN", new KeyValuePair<Xbox360Axes, short>(Xbox360Axes.LeftThumbY, negAxisValue));
            axesDict.Add("RLEFT", new KeyValuePair<Xbox360Axes, short>(Xbox360Axes.RightThumbX, negAxisValue));
            axesDict.Add("RRIGHT", new KeyValuePair<Xbox360Axes, short>(Xbox360Axes.RightThumbX, posAxisValue));
            axesDict.Add("RUP", new KeyValuePair<Xbox360Axes, short>(Xbox360Axes.RightThumbY, posAxisValue));
            axesDict.Add("RDOWN", new KeyValuePair<Xbox360Axes, short>(Xbox360Axes.RightThumbY, negAxisValue));

        }

        private void InitializeButtonsDict()
        {
            buttonsDict.Add("UP", Xbox360Buttons.Up);
            buttonsDict.Add("DOWN", Xbox360Buttons.Down);
            buttonsDict.Add("LEFT", Xbox360Buttons.Left);
            buttonsDict.Add("RIGHT", Xbox360Buttons.Right);
            buttonsDict.Add("A", Xbox360Buttons.A);
            buttonsDict.Add("B", Xbox360Buttons.B);
            buttonsDict.Add("X", Xbox360Buttons.X);
            buttonsDict.Add("Y", Xbox360Buttons.Y);
            buttonsDict.Add("START", Xbox360Buttons.Start);
            buttonsDict.Add("BACK", Xbox360Buttons.Back);
            buttonsDict.Add("GUIDE", Xbox360Buttons.Guide);
            buttonsDict.Add("LB", Xbox360Buttons.LeftShoulder);
            buttonsDict.Add("LTB", Xbox360Buttons.LeftThumb);
            buttonsDict.Add("RB", Xbox360Buttons.RightShoulder);
            buttonsDict.Add("RTB", Xbox360Buttons.RightThumb);

        }
        public void Close()
        {
            focusTimer.Stop();
            focusTimer.Dispose();
            skatePulseTimer.Stop();
            skatePulseTimer.Dispose();
            ollieTimer.Stop();
            ollieTimer.Dispose();
            log.Info("Closing");
            foreach (Xbox360Controller controller in controllers)
            {
                log.Debug(String.Format("Disconnecting {0}", controller.ToString()));
                controller.Disconnect();
            }
            log.Debug("Disposing of ViGEm client");
            client.Dispose();
        }
        public void Enable()
        {
            enabled = true;
            NotifyListeners(enabled);
        }
        public void ReloadMapping(string mappingFile)
        {
            Config next = new Config(mappingFile);
            if (next.PadCount != controllers.Count)
                throw new InvalidOperationException("The new mapping must keep the same controller count.");
            ResetReports();
            config = next;
        }
        public void Disable()
        {
            enabled = false;
            ResetReports();
            NotifyListeners(enabled);
        }
        public void ToggleEnabled()
        {
            enabled = !enabled;
            if (!enabled) ResetReports();
            NotifyListeners(enabled);
        }
        public Boolean IsEnabled()
        {
            return enabled;
        }
    }
}
