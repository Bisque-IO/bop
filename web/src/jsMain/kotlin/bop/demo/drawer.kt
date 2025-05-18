package bop.demo

import bop.ui.*
import react.FC
import react.dom.html.ReactHTML.div
import react.dom.html.ReactHTML.h4
import react.dom.html.ReactHTML.p
import react.useCallback
import react.useState
import web.cssom.ClassName
import kotlin.math.max
import kotlin.math.min

val DrawerDemo = FC {
   div {
      className = ClassName("flex flex-wrap items-start gap-4")
      DrawerBottom {}
      DrawerScrollableContent {}
   }
}

val DrawerBottom = FC {
   val (goal, setGoal) = useState(350)

   val onClick = useCallback { adjustment: Number ->
      setGoal({ max(200, min(400, it + adjustment.toInt())) })
   }

   Drawer {
      DrawerTrigger {
         asChild = true

         Button {
            variant = "outline"
            +"Open Drawer"
         }
      }

      DrawerContent {
         div {
            className = ClassName("mx-auto w-full max-w-sm")

            DrawerHeader {
               DrawerTitle {
                  +"Move Goal"
               }
               DrawerDescription {
                  +"Set your daily activity goal."
               }
            }
         }
      }
   }
}

val DrawerScrollableContent = FC {
   Drawer {
      direction = "right"
      DrawerTrigger {
         asChild = true

         Button {
            variant = "outline"
            +"Scrollable Content"
         }
      }
      DrawerContent {
         DrawerHeader {
            DrawerTitle {
               className = ClassName("text-lg")
               +"Move Goal"
            }
            DrawerDescription {
               +"Set your daily activity goal."
            }
         }
         div {
            className = ClassName("overflow-y-auto px-4 text-sm")
            h4 {
               className = ClassName("mb-4 text-md leading-none font-normal")
               repeat(15) {
                  p {
                     key = it.toString()
                     className = ClassName("mb-4 leading-normal")
                     +"Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do\n"
                     +"eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut\n"
                     +"enim ad minim veniam, quis nostrud exercitation ullamco laboris\n"
                     +"nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in\n"
                     +"reprehenderit in voluptate velit esse cillum dolore eu fugiat\n"
                     +"nulla pariatur. Excepteur sint occaecat cupidatat non proident,\n"
                     +"sunt in culpa qui officia deserunt mollit anim id est laborum."
                  }
               }
            }
         }
         DrawerFooter {
            Button {
               variant = "secondary"
               +"Submit"
            }
            DrawerClose {
               asChild = true
               Button {
                  variant = "outline"
                  +"Cancel"
               }
            }
         }
      }
   }
}