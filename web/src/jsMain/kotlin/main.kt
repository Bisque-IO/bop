

//import kotlinx.browser.document
import bop.ui.Button
import js.objects.unsafeJso
import lucide.ArrowLeftIcon
import react.*
import react.dom.html.ReactHTML.h1

import react.FC
import react.Props
import react.dom.client.createRoot


import react.dom.client.RootOptions

import web.dom.document

import radix.ui.AccordionModule
import radix.ui.ActivityLogIcon
import radix.ui.BarChartIcon
import radix.ui.DialogClose
import radix.ui.DialogContent
import radix.ui.DialogDescription
import radix.ui.DialogOverlay
import radix.ui.DialogPortal
import radix.ui.DialogRoot
import radix.ui.DialogTitle
import radix.ui.DialogTrigger
import radix.ui.PopoverArrow
import radix.ui.PopoverClose
import radix.ui.PopoverContent
import radix.ui.PopoverRoot
import radix.ui.PopoverTrigger
import radix.ui.Select
import radix.ui.SelectContent
import radix.ui.SelectItem
import radix.ui.SelectTrigger
import radix.ui.SelectValue
import radix.ui.SelectViewport
import react.dom.html.ReactHTML.button
import react.dom.html.ReactHTML.div
import react.router.Outlet
import react.router.dom.RouterProvider
import react.router.dom.createBrowserRouter
import react.router.useLoaderData
import react.router.useNavigate
import web.cssom.ClassName
import web.sockets.WebSocket

val Accordion = AccordionModule.Accordion

val MyDialog = FC {
    val (isOpen, setOpen) = useState(false)

    DialogRoot {
        open = isOpen
        onOpenChange = { setOpen(it) }

        DialogTrigger {
            +"Open Dialog"
        }

        DialogPortal {
            DialogOverlay {
                className = ClassName("fixed inset-0 bg-black bg-opacity-50")
            }

            DialogContent {
                className = ClassName("fixed top-1/2 left-1/2 bg-white p-6 rounded-md transform -translate-x-1/2 -translate-y-1/2 w-[300px]")

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

val MySelect = FC {
    val (selected, setSelected) = useState("apple")

    Select {
        value = selected
        onValueChange = { setSelected(it) }

        SelectTrigger {
            className = ClassName("inline-flex items-center justify-between border px-3 py-2 w-48")
            SelectValue { placeholder = "Select a fruit" }
        }

        SelectContent {
            SelectViewport {
                listOf("apple", "banana", "mango").forEach {
                    SelectItem {
                        value = it
                        +it.replaceFirstChar(Char::uppercase)
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
            Button {
                variant = "outline"
                className = ClassName("m-6")
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
                radix.ui.HomeIcon {}
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
        Accordion {
            +"Click Me"
        }
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
        }
    )
)

private val App = FC<Props> {
    div {
        className = ClassName("p-6 bg-black-900 text-white rounded")
        +"App"
        Outlet
        RouterProvider {
            router = BrowserRouter
        }
    }
}

private val Root = FC<Props> {
    StrictMode {
        App {

        }
    }
}

fun main() {
//    TailwindStyles
    kotlinx.browser.document.addEventListener("DOMContentLoaded", {
//        val root = document.createElement("div")
//        document.body.appendChild(root)
//        console.log("router", BrowserRouter)
        createRoot(document.body, RootOptions())
            .render(createElement(Root))

    })
}