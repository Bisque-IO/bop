package bop.ui

import lib.sonner.ToasterProps
import react.FC

val Toaster = FC<ToasterProps> { props ->
   val style = js("{}")
   style["--normal-bg"] = "var(--popover)"
   style["--normal-text"] = "var(--popover-foreground)"
   style["--normal-border"] = "var(--border)"
   lib.sonner.Toaster {
      +props
      className = cn("toaster group")
      this.style = style
   }
}