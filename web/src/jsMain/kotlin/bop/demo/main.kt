package bop.demo

import bop.ui.*
import js.objects.unsafeJso
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
   val navigate = useNavigate()

   div {
//      div {
//         +"Home Page: Loaded message: $data"
//      }
//
//      div {
//         Checkbox {}
//      }
//
//      div {
//         className = ClassName("pt-6")
//         Button {
//            variant = "outline"
//            className = ClassName("p-2")
//            onClick = { navigate("/about") }
//            +"Outline!!"
//         }
//         Button {
//            variant = "ghost"
//            onClick = { navigate("/about") }
//            +"Ghost"
//         }
//         Button {
//            variant = "destructive"
//            onClick = { navigate("/about") }
//            +"Destructive"
//         }
//         Button {
//            variant = "secondary"
//            onClick = { navigate("/about") }
//            +"Secondary"
//         }
//         Button {
//            variant = "link"
//            className = ClassName("m-6")
//            onClick = { navigate("/about") }
//            +".Link"
//         }
//         Button {
//            variant = "outline"
//            onClick = { navigate("/about") }
//            HomeIcon {}
//            +".Home"
//         }
//
//         Button {
//            variant = "outline"
//            size = "lg"
//            onClick = { navigate("/about") }
//            ArrowLeftIcon {}
//            +"Go Back!"
//         }
//      }

      p {
         className = cn("p-10")
         style = unsafeJso { margin = 15.px }
      }

      div {
         DropdownMenuSimple {}
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
         //        val root = document.createElement("div")
         //        document.body.appendChild(root)
         //        console.log("router", .BrowserRouter)
         createRoot(
            document.body, RootOptions()
         ).render(createElement(Root))
      },
   )
}
