package bop.demo

import bop.ui.ResizableHandle
import bop.ui.ResizablePanel
import bop.ui.ResizablePanelGroup
import bop.ui.cn
import react.FC
import react.dom.html.ReactHTML.div
import react.dom.html.ReactHTML.span

val ResizableDemo = FC {
   div {
      className = cn("flex w-full flex-col gap-6")

      ResizablePanelGroup {
         direction = "horizontal"
         className = cn("rounded-lg border md:min-w-[450px]")

         ResizablePanel {
            defaultSize = 50
            div {
               className = cn("flex h-[200px] items-center justify-center p-6")
               span {
                  className = cn("font-semibold")
                  +"One"
               }
            }
         }
         ResizableHandle {}
         ResizablePanel {
            defaultSize = 50
            ResizablePanelGroup {
               direction = "vertical"
               ResizablePanel {
                  defaultSize = 25
                  div {
                     className = cn("flex h-full items-center justify-center p-6")
                     span {
                        className = cn("font-semibold")
                        +"Two"
                     }
                  }
               }
               ResizableHandle {}
               ResizablePanel {
                  defaultSize = 75
                  div {
                     className = cn("flex h-full items-center justify-center p-6")
                     span {
                        className = cn("font-semibold")
                        +"Three"
                     }
                  }
               }
            }
         }
      }

      ResizablePanelGroup {
         direction = "horizontal"
         className = cn("min-h-[200px] max-w-lg rounded-lg border md:min-w-[450px]")

         ResizablePanel {
            defaultSize = 25
            div {
               className = cn("flex h-full items-center justify-center p-6")
               span {
                  className = cn("font-semibold")
                  +"Sidebar"
               }
            }
         }
         ResizableHandle { withHandle = true }
         ResizablePanel {
            defaultSize = 75
            div {
               className = cn("flex h-full items-center justify-center p-6")
               span {
                  className = cn("font-semibold")
                  +"Content"
               }
            }
         }
      }

      ResizablePanelGroup {
         direction = "vertical"
         className = cn("min-h-[200px] max-w-md rounded-lg border md:min-w-[450px]")

         ResizablePanel {
            defaultSize = 25
            div {
               className = cn("flex h-full items-center justify-center p-6")
               span {
                  className = cn("font-semibold")
                  +"Header"
               }
            }
         }
         ResizableHandle { withHandle = true }
         ResizablePanel {
            defaultSize = 75
            div {
               className = cn("flex h-full items-center justify-center p-6")
               span {
                  className = cn("font-semibold")
                  +"Content"
               }
            }
         }
      }
   }
}