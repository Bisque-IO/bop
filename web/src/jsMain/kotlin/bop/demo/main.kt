package bop.demo

import bop.ui.*
import js.objects.unsafeJso
import lib.lucide.ArrowLeftIcon
import lib.lucide.HomeIcon
import react.FC
import react.Props
import react.createElement
import react.dom.client.RootOptions
import react.dom.client.createRoot
import react.dom.html.ReactHTML.div
import react.dom.html.ReactHTML.p
import react.router.Outlet
import react.router.dom.RouterProvider
import react.router.dom.createBrowserRouter
import react.router.useLoaderData
import react.router.useNavigate
import web.cssom.ClassName
import web.cssom.px
import web.dom.document
import web.html.InputType

val Home = FC {
   val data = useLoaderData()

   div {
      div {
         +"Home Page: Loaded message: $data"
      }

      div {
         Checkbox {}
      }

      div {
         ButtonDemo {}
      }

      p {
         className = cn("p-10")
         style = unsafeJso { margin = 15.px }
      }

      div {
         DropdownMenuSimple {}
      }

      div {
         className = ClassName("pt-6")
         CommandDemo {}
      }

      div {
         className = ClassName("pt-6")
         DrawerDemo {}
      }

      div {
         className = ClassName("pt-6")
         Alert {
            //                variant = "destructive"
            AlertTitle { +"Alert Title" }
            AlertDescription { +"This is a description about the alert" }
         }
      }

      div {
         className = ClassName("pt-6")
         Input { type = InputType.text }
         Input { type = InputType.password }
      }

      InputOTPDemo {}

      div {
         className = ClassName("pt-6")
         SelectDemo {}
      }

      div {
         className = ClassName("pt-6")

         MenubarDemo {}
      }

      div {
         className = ClassName("pt-6")
         NavMenuDemo {}
      }

      div {
         className = ClassName("pt-6")
         PopoverDemo {}
      }

      div {
         className = ClassName("pt-6")
         ProgressDemo {}
      }

      div {
         className = ClassName("pt-6")
         RadioGroupDemo {}
      }

      div {
         className = ClassName("pt-6")
         ScrollAreaDemo {}
      }


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
      className = cn("py-6 p-6")
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
//   val scope = MainScope()
//   scope.launch {
//      for (i in 0..1000) {
//         delay(1000)
//         console.log(i.toString())
//      }
//   }
   //    TailwindStyles
   kotlinx.browser.document.addEventListener(
      "DOMContentLoaded",

      {
//         console.log(document.body.className)
//         document.body.className += document.body.className + "theme-bop"
//         console.log(document.body.className)
         //        val root = document.createElement("div")
         //        document.body.appendChild(root)
         //        console.log("router", .BrowserRouter)
         createRoot(
            document.body, RootOptions()
         ).render(createElement(Root))
      },
   )
}
