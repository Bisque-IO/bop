import bop.ui.Accordion
import bop.ui.Alert
import bop.ui.AlertDescription
import bop.ui.AlertTitle
import bop.ui.Button
import bop.ui.Checkbox
import bop.ui.Dialog
import bop.ui.DialogClose
import bop.ui.DialogContent
import bop.ui.DialogDescription
import bop.ui.DialogOverlay
import bop.ui.DialogPortal
import bop.ui.DialogTitle
import bop.ui.DialogTrigger
import bop.ui.DropdownMenu
import bop.ui.DropdownMenuContent
import bop.ui.DropdownMenuGroup
import bop.ui.DropdownMenuItem
import bop.ui.DropdownMenuLabel
import bop.ui.DropdownMenuSeparator
import bop.ui.DropdownMenuShortcut
import bop.ui.DropdownMenuTrigger
import js.objects.unsafeJso
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import lucide.ArrowLeftIcon

import react.FC
import react.Props
import react.createElement
import react.dom.client.RootOptions
import react.dom.client.createRoot
import react.dom.html.ReactHTML.button
import react.dom.html.ReactHTML.div
import react.dom.html.ReactHTML.h1
import react.dom.html.ReactHTML.p
import react.router.Outlet
import react.router.dom.RouterProvider
import react.router.dom.createBrowserRouter
import react.router.useLoaderData
import react.router.useNavigate
import react.useState
import web.cssom.ClassName
import web.cssom.px
import web.dom.document

val MyDialog = FC {
   val (isOpen, setOpen) = useState(false)

   Dialog {
      open = isOpen
      onOpenChange = { setOpen(it) }

      DialogTrigger { +"Open Dialog" }

      DialogPortal {
         DialogOverlay { className = ClassName("fixed inset-0 bg-black bg-opacity-50") }

         DialogContent {
            className = ClassName(
               "fixed top-1/2 left-1/2 bg-white p-6 rounded-md transform -translate-x-1/2 -translate-y-1/2 w-[300px]"
            )

            DialogTitle {
               className = ClassName("text-lg font-bold mb-2")
               +"Dialog Title"
            }

            DialogDescription {
               className = ClassName("mb-4")
               +"Dialog is open: $isOpen"
            }

            DialogClose {
               button {
                  className = ClassName("mt-2 bg-blue-500 text-white px-4 py-2 rounded")
                  +"Close"
               }
            }
         }
      }
   }
}

val DropdownMenuSimple = FC {
   DropdownMenu {
      defaultOpen = false
      DropdownMenuTrigger {
         asChild = true

         Button {
            variant = "outline"
            +"Open"
         }
      }

      DropdownMenuContent {
         align = "start"
         className = ClassName("w-56")

         DropdownMenuLabel {
            +"MyAccount"
         }
         DropdownMenuSeparator {}
         DropdownMenuGroup {
            DropdownMenuItem {
               +"Profile"
               DropdownMenuShortcut {
                  +"⇧⌘P"
               }
            }
            DropdownMenuItem {
               +"Billing"
               DropdownMenuShortcut {
                  +"⌘B"
               }
            }

            DropdownMenuItem {
               +"Settings"
               DropdownMenuShortcut {
                  +"⌘S"
               }
            }
            DropdownMenuItem {
               +"Keyboard shortcuts"
               DropdownMenuShortcut {
                  +"⌘K"
               }
            }
         }
      }
   }
}

val Home = FC {
   val data = useLoaderData()
   val navigate = useNavigate()

   div {
      div {
         //            className = ClassName("text-3xl font-bold underline")
         +"Home Page: Loaded message: $data"
         //            MyDialog {}
      }

      div {
         Checkbox {}
      }

      div {
         Button {
            variant = "outline"
            className = ClassName("p-2")
            onClick = { navigate("/about") }
            +"Outline!!"
         }
         Button {
            variant = "ghost"
            onClick = { navigate("/about") }
            +"Ghost"
         }
         Button {
            variant = "destructive"
            onClick = { navigate("/about") }
            +"Destructive"
         }
         Button {
            variant = "secondary"
            onClick = { navigate("/about") }
            +"Secondary"
         }
         Button {
            variant = "link"
            className = ClassName("m-6")
            onClick = { navigate("/about") }
            +"Link"
         }
         Button {
            variant = "outline"
            onClick = { navigate("/about") }
            lucide.HomeIcon {}
            +"Home"
         }

         Button {
            variant = "outline"
            size = "lg"
            onClick = { navigate("/about") }
            ArrowLeftIcon {}
            +"Go Back!"
         }
      }

      p {
         className = ClassName("p-10")
         style = unsafeJso { margin = 15.px }
      }

      DropdownMenuSimple {}

      DrawerDemo {}

      div {
         Alert {
            //                variant = "destructive"
            AlertTitle { +"Alert Title" }
            AlertDescription { +"This is a description about the alert" }
         }
      }


      //        div { ActivityLogIcon {} }
      //        div { BarChartIcon {} }
      //        div { MySelect {} }
   }
}

val About = FC {
   div { +"About Page" }
}

val HomeLoader: dynamic = {
   println("Loader running")
   "Hello from Kotlin Loader"
}

val ErrorElement = FC {
   div { +"Something went wrong" }
}

external interface WelcomeProps : Props {
   var name: String
}

private val Welcome = FC<WelcomeProps>("Welcome") { props ->
   h1 {
      +"Hello, ${props.name}"
      Accordion { +"Click Me" }
   }
}

val BrowserRouter = createBrowserRouter(
   arrayOf(
      unsafeJso {
         path = "/"
         element = createElement(Home)
         loader = HomeLoader
         errorElement = createElement(ErrorElement)
      },
      unsafeJso {
         path = "/about"
         element = createElement(About)
      },
   )
)

private val App = FC<Props> {
   div {
      className = ClassName("py-4 p-6 bg-black-900 text-white rounded")
      +"App"
      Outlet
      RouterProvider { router = BrowserRouter }
   }
}

private val Root = FC<Props> {
   //    StrictMode {
   App {}
   //    }
}

fun main() {
   val scope = MainScope()
   scope.launch {
      for (i in 0..1000) {
         delay(1000)
         console.log(i.toString())
      }
   }
   //    TailwindStyles
   kotlinx.browser.document.addEventListener(
      "DOMContentLoaded",
      {
         //        val root = document.createElement("div")
         //        document.body.appendChild(root)
         //        console.log("router", BrowserRouter)
         createRoot(
            document.body, RootOptions()
         ).render(createElement(Root))
      },
   )
}
