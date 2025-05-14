package bop.ui

import bop.ui.utils.cn
import bop.ui.utils.set
import bop.ui.utils.spread

import react.FC
import react.dom.html.HTMLAttributes
import react.dom.html.ReactHTML.div
import web.html.HTMLDivElement

val Card = FC<HTMLAttributes<HTMLDivElement>> { props ->
    div {
        this["data-slot"] = "card"
        className =
            cn("bg-card text-card-foreground flex flex-col gap-6 rounded-xl border py-6 shadow-sm", props.className)
        spread(props, "className")
    }
}


val CardHeader = FC<HTMLAttributes<HTMLDivElement>> { props ->
    div {
        this["data-slot"] = "card-header"
        className = cn(
            "@container/card-header grid auto-rows-min grid-rows-[auto_auto] items-start gap-1.5 px-6 has-data-[slot=card-action]:grid-cols-[1fr_auto] [.border-b]:pb-6",
            props.className
        )
        spread(props, "className")
        props.children?.let { +it }
    }
}


val CardTitle = FC<HTMLAttributes<HTMLDivElement>> { props ->
    div {
        this["data-slot"] = "card-title"
        className = cn("leading-none font-semibold", props.className)
        spread(props, "className")
        props.children?.let { +it }
    }
}


val CardDescription = FC<HTMLAttributes<HTMLDivElement>> { props ->
    div {
        this["data-slot"] = "card-description"
        className = cn("text-muted-foreground text-sm", props.className)
        spread(props, "className")
        props.children?.let { +it }
    }
}


val CardAction = FC<HTMLAttributes<HTMLDivElement>> { props ->
    div {
        this["data-slot"] = "card-action"
        className = cn("col-start-2 row-span-2 row-start-1 self-start justify-self-end", props.className)
        spread(props, "className")
        props.children?.let { +it }
    }
}


val CardContent = FC<HTMLAttributes<HTMLDivElement>> { props ->
    div {
        this["data-slot"] = "card-content"
        className = cn("px-6", props.className)
        spread(props, "className")
        props.children?.let { +it }
    }
}


val CardFooter = FC<HTMLAttributes<HTMLDivElement>> { props ->
    div {
        set("data-slot", "card-footer")
        className = cn("flex items-center px-6 [.border-t]:pt-6", props.className)
        spread(props, "className")
        props.children?.let { +it }
    }
}
