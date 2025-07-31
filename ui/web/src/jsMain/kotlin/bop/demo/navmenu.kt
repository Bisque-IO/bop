package bop.demo

import bop.ui.*
import js.objects.unsafeJso
import kotlinx.browser.window
import lib.lucide.CircleIcon
import lib.radix.CheckCircledIcon
import react.FC
import react.dom.html.ReactHTML.a
import react.dom.html.ReactHTML.div
import react.dom.html.ReactHTML.li
import react.dom.html.ReactHTML.p
import react.dom.html.ReactHTML.ul
import web.cssom.ClassName

external interface ComponentDesc {
   var title: String
   var href: String
   var description: String
}

var components: Array<ComponentDesc> = arrayOf(
   unsafeJso {
      title = "Alert Dialog"
      href = "/docs/primitives/alert-dialog"
      description = "A modal dialog that interrupts the user with important content and expects a response."
   },
   unsafeJso {
      title = "Hover Card"
      href = "/docs/primitives/hover-card"
      description = "For sighted users to preview content available behind a link."
   },
   unsafeJso {
      title = "Progress"
      href = "/docs/primitives/progress"
      description =
         "Displays an indicator showing the completion progress of a task, typically displayed as a progress bar."
   },
   unsafeJso {
      title = "Scroll-area"
      href = "/docs/primitives/scroll-area"
      description = "Visually or semantically separates content."
   },
   unsafeJso {
      title = "Tabs"
      href = "/docs/primitives/tabs"
      description = "A set of layered sections of content—known as tab panels—that are displayed one at a time."
   },
   unsafeJso {
      title = "Tooltip"
      href = "/docs/primitives/tooltip"
      description =
         "A popup that displays information related to an element when the element receives keyboard focus or the mouse hovers over it."
   },
)

val NavMenuDemo = FC {
   div {
      className = ClassName("w-full flex-col items-center justify-center gap-6 @xl:flex")

      NavMenu {
         viewport = true
         delayDuration = 1
         NavMenuList {
            NavMenuItem {
               NavMenuTrigger { +"Getting started" }
               NavMenuContent {
                  ul {
                     className = ClassName("grid gap-2 md:w-[400px] lg:w-[500px] lg:grid-cols-[.75fr_1fr]")
                     li {
                        className = ClassName("row-span-3")

                        NavMenuLink {
                           asChild = true

                           a {
                              className =
                                 ClassName("from-muted/50 to-muted flex h-full w-full flex-col justify-end rounded-md bg-linear-to-b p-6 no-underline outline-hidden select-none focus:shadow-md")
                              href = "/"

                              div {
                                 className = ClassName("mt-4 text-lg font-medium")
                                 +"bop/ui"
                              }
                              div {
                                 className = ClassName("mb-2 text-lg font-medium")
                                 +"shadcn/ui"
                              }
                              p {
                                 className = ClassName("text-muted-foreground text-sm leading-tight")
                                 +"Beautifully designed .components built with Tailwind CSS."
                              }
                           }
                        }
                     }
                     ListItem {
                        href = "/docs"
                        title = "Introduction"
                        +"Re-usable .components built using Radix UI and Tailwind CSS."
                     }
                     ListItem {
                        href = "/docs/installation"
                        title = "Installation"
                        +"How to install dependencies and structure your app."
                     }
                     ListItem {
                        href = "/docs/primitives/typography"
                        title = "Typography"
                        +"Styles for headings, paragraphs, lists...etc"
                     }
                  }
               }
            }

            NavMenuItem {
               NavMenuTrigger { +"Components" }
               NavMenuContent {
                  ul {
                     className = ClassName("grid w-[400px] gap-2 md:w-[500px] md:grid-cols-2 lg:w-[600px]")

                     components.forEach {
                        ListItem {
                           key = it.title
                           title = it.title
                           href = it.href

                           +it.description
                        }
                     }
                  }
               }
            }

            NavMenuItem {
               NavMenuLink {
                  asChild = true
                  className = ClassName(navigationMenuTriggerStyle())

                  Link {
                     onClick = { window.location.href = "javascript:void(0);" }
                     +"Documentation"
                  }
               }
            }
         }
      }

      NavMenu {
         viewport = false

         NavMenuList {
            NavMenuItem {
               NavMenuLink {
                  asChild = true
                  className = ClassName(navigationMenuTriggerStyle())

                  Link {
                     onClick = { window.location.href = "javascript:void(0);" }
                     +"Documentation"
                  }
               }
            }

            NavMenuItem {
               NavMenuTrigger { +"List" }
               NavMenuContent {
                  ul {
                     className = ClassName("grid w-[300px] gap-4")

                     li {
                        NavMenuLink {
                           asChild = true
                           Link {
                              onClick = { window.location.href = "javascript:void(0);" }

                              div {
                                 className = ClassName("font-medium")
                                 +"Components"
                              }
                              div {
                                 className = ClassName("text-muted-foreground")
                                 +"Browse all .components in the library."
                              }
                           }
                        }
                        NavMenuLink {
                           asChild = true
                           Link {
                              onClick = { window.location.href = "javascript:void(0);" }

                              div {
                                 className = ClassName("font-medium")
                                 +"Documentation"
                              }
                              div {
                                 className = ClassName("text-muted-foreground")
                                 +"Learn how to use the library."
                              }
                           }
                        }
                        NavMenuLink {
                           asChild = true
                           Link {
                              onClick = { window.location.href = "javascript:void(0);" }

                              div {
                                 className = ClassName("font-medium")
                                 +"Blog"
                              }
                              div {
                                 className = ClassName("text-muted-foreground")
                                 +"Read our latest blog posts."
                              }
                           }
                        }
                     }
                  }
               }
            }

            NavMenuItem {
               NavMenuTrigger { +"Simple List" }
               NavMenuContent {
                  ul {
                     className = ClassName("grid w-[200px] gap-4")

                     li {
                        NavMenuLink {
                           asChild = true
                           Link {
                              onClick = { window.location.href = "javascript:void(0);" }
                              +"Components"
                           }
                        }
                        NavMenuLink {
                           asChild = true
                           Link {
                              onClick = { window.location.href = "javascript:void(0);" }
                              +"Documentation"
                           }
                        }
                        NavMenuLink {
                           asChild = true
                           Link {
                              onClick = { window.location.href = "javascript:void(0);" }
                              +"Blocks"
                           }
                        }
                     }
                  }
               }
            }

            NavMenuItem {
               NavMenuTrigger { +"With Icon" }
               NavMenuContent {
                  ul {
                     className = ClassName("grid w-[200px] gap-4")

                     li {
                        NavMenuLink {
                           asChild = true

                           Link {
                              className = ClassName("flex-row items-center gap-2")
                              onClick = { window.location.href = "javascript:void(0);" }

                              CheckCircledIcon {}
                              +"Done"
                           }
                        }
                        NavMenuLink {
                           asChild = true
                           Link {
                              className = ClassName("flex-row items-center gap-2")
                              onClick = { window.location.href = "javascript:void(0);" }

                              CircleIcon {}
                              +"To Do"
                           }
                        }
                        NavMenuLink {
                           asChild = true
                           Link {
                              className = ClassName("flex-row items-center gap-2")
                              onClick = { window.location.href = "javascript:void(0);" }

                              CheckCircledIcon {}
                              +"Done"
                           }
                        }
                     }
                  }
               }
            }
         }
      }
   }
}

val Link = FC<DefaultProps>("Link") { props ->
   a {
      +props
   }
}

external interface ListItemProps : DefaultProps {
   var href: String?
}

val ListItem = FC<ListItemProps>("ListItem") { props ->
   li {
      +props
      children = null
      NavMenuLink {
         asChild = true
         Link {
            onClick = { window.location.href = props.href ?: "" }
            div {
               className = ClassName("text-sm leading-none font-medium")
               +props.title
            }
            p {
               className = ClassName("text-muted-foreground line-clamp-2 text-sm leading-snug")
               +props.children
            }
         }
      }
   }
}