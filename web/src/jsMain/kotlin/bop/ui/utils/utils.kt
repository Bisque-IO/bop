package bop.ui.utils

import react.Props
import react.dom.html.HTMLAttributes
import web.html.HTMLElement

operator fun <E: HTMLElement> HTMLAttributes<E>.set(name: String, value: dynamic) {
    asDynamic()[name] = value
}

operator fun <P: Props> P.set(name: String, value: dynamic) {
    asDynamic()[name] = value
}

fun <P0: Props, P1: Props> P0.spread(props: P1, exclude: String = ""): P0 {
    val dynamicProps = props.asDynamic()
    js("Object").keys(dynamicProps).unsafeCast<Array<String>>().forEach { key ->
        if (key != exclude) {
            val value = dynamicProps[key]
            if (value != undefined) {
                asDynamic()[key] = value
            }
        }
    }
    return this
}

fun <P0: Props, P1: Props> P0.spread(props: P1, exclude: Set<String> = emptySet()) {
    val dynamicProps = props.asDynamic()
    js("Object").keys(dynamicProps).unsafeCast<Array<String>>().forEach { key ->
        if (key !in exclude) {
            val value = dynamicProps[key]
            if (value != undefined) {
                asDynamic()[key] = value
            }
        }
    }
}