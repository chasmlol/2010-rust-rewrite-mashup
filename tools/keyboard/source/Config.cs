using IniParser;
using IniParser.Model;
using Nefarius.ViGEm.Client.Targets.Xbox360;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Keyboard2XinputLib
{
    class Config
    {
        private static readonly log4net.ILog log = log4net.LogManager.GetLogger(System.Reflection.MethodBase.GetCurrentMethod().DeclaringType);
        private static readonly String DEFAULT_NAME = "mapping.ini";

        public List<IniData> Mappings { get; private set; }
        private int currentMappingIndex;
        public int CurrentMappingIndex {
            get { return currentMappingIndex; }
            set {
                if (value + 1 > Mappings.Count)
                {
                    log.Warn(String.Format("Mapping{0} doesn't exist; active mapping not changed", value));
                } else
                {
                    currentMappingIndex = value;
                }

            }
        }
        public int PadCount { get; private set; }

        public Config(string configFilePath)
        {
            Mappings = new List<IniData>(0);
            if (configFilePath == null)
            {
                configFilePath = DEFAULT_NAME;
                log.Debug(String.Format("Using default config file: {0}", configFilePath));
            }
            if (!System.IO.Path.IsPathRooted(configFilePath))
            {
                // get the directory where the program resides
                string codebase = System.Reflection.Assembly.GetExecutingAssembly().CodeBase;
                string baseDir = new Uri(System.IO.Path.GetDirectoryName(codebase)).LocalPath;
                configFilePath = baseDir + "\\" + configFilePath;
            }

            if (!System.IO.File.Exists(configFilePath))
            {
                //log.Error(String.Format("Config file does not exist: {0}", configFilePath));
                throw new FileNotFoundException(String.Format("Config file does not exist: {0}", configFilePath));
            }

            // read config(s)
            var parser = new FileIniDataParser();
            log.Info(String.Format("Loading config file: {0}", configFilePath));
            Mappings.Add(parser.ReadFile(configFilePath));
            // how many pads?
            PadCount = Math.Max(PadCount, countPads(Mappings[0]));

            // additional mappings
            string baseFolder = System.IO.Path.GetDirectoryName(configFilePath);
            int i = 1;
            Boolean exists;
            do {
                configFilePath = String.Format("{0}\\mapping{1}.ini", baseFolder, i);
                exists = System.IO.File.Exists(configFilePath);
                if (exists)
                {
                    log.Info(String.Format("Loading additional mapping file: {0}", configFilePath));
                    Mappings.Add(parser.ReadFile(configFilePath));
                    if (Mappings[i]["config"].Count > 0)
                    {
                        throw new Exception(String.Format("Additional mapping file {0} must NOT contain a 'config' section", configFilePath));
                    }
                    if (Mappings[i]["startup"].Count > 0)
                    {
                        throw new Exception(String.Format("Additional mapping file {0} must NOT contain a 'startup' section", configFilePath));
                    }
                    // update pad count
                    PadCount = Math.Max(PadCount, countPads(Mappings[i]));
                    // copy the config from mapping 0
                    Mappings[i]["config"].Merge(Mappings[0]["config"]);
                }
                i++;
            } while (exists);

            log.Info(String.Format("found {0} pads", PadCount));
            log.Info(String.Format("found {0} mappings", Mappings.Count));
        }

        public IniData getCurrentMapping()
        {
            return Mappings[currentMappingIndex];
        }

        private int countPads(IniData mapping)
        {
            int result = 0;
            foreach (SectionData section in mapping.Sections)
            {
                String intStr = section.SectionName.Substring(section.SectionName.Length - 1);
                int padNumber = 0;
                if (int.TryParse(intStr, out padNumber))
                {
                    log.Debug(String.Format("found config for pad {0}", padNumber));
                    result = Math.Max(PadCount, padNumber);

                }
                else if (("config".Equals(section.SectionName)) || ("startup".Equals(section.SectionName)) || ("unassigned".Equals(section.SectionName)))
                {
                    // nothing special?
                }
                else
                {
                    log.Error(String.Format("Ignored section [{0}]", section.SectionName));
                }
            }
            return result;
        }
    }
}
