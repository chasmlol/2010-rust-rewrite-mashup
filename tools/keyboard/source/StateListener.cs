using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Keyboard2XinputLib
{
    public interface StateListener
    {
        void NotifyEnabled(Boolean enabled);
    }
}
