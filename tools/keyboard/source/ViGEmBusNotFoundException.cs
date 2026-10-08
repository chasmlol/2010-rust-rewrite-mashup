using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.Serialization;
using System.Text;
using System.Threading.Tasks;

namespace Keyboard2XinputLib.Exceptions
{
    /**
     * Class created so that ViGEm API does not leak into the GUI
     */
    public class ViGEmBusNotFoundException : Exception
    {
        public ViGEmBusNotFoundException()
        {
        }

        public ViGEmBusNotFoundException(string message) : base(message)
        {
        }

        public ViGEmBusNotFoundException(string message, Exception innerException) : base(message, innerException)
        {
        }

        protected ViGEmBusNotFoundException(SerializationInfo info, StreamingContext context) : base(info, context)
        {
        }
    }
}
