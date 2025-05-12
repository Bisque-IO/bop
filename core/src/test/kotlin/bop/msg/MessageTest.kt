package bop.msg

import bop.bench.Bench
import bop.io.BytesMut
import bop.io.DirectBytes
import org.junit.jupiter.api.Test
import java.lang.invoke.MethodHandles
import java.lang.reflect.Modifier
import java.util.function.Function
import java.util.stream.Stream

@Retention(AnnotationRetention.SOURCE)
@Target(AnnotationTarget.VALUE_PARAMETER)
annotation class LineNumber

@Retention(AnnotationRetention.SOURCE)
@Target(AnnotationTarget.VALUE_PARAMETER)
annotation class MethodName

@Retention(AnnotationRetention.SOURCE)
@Target(AnnotationTarget.VALUE_PARAMETER)
annotation class CallerLineNumber

@Retention(AnnotationRetention.SOURCE)
@Target(AnnotationTarget.VALUE_PARAMETER)
annotation class CallerMethod

@Retention(AnnotationRetention.SOURCE)
@Target(AnnotationTarget.VALUE_PARAMETER)
annotation class CallerClass

@Retention(AnnotationRetention.SOURCE)
@Target(AnnotationTarget.LOCAL_VARIABLE)
annotation class LOG


fun lineNumber(@CallerLineNumber lineNumber: Int) = lineNumber

fun log(
    @CallerClass callerClass: String = "",
    @CallerMethod callerMethod: String = "",
    @CallerLineNumber callerLineNumber: Int = 0
) {

}

fun doLog() {
    log()

    object : Block {
        override val size: Int = 0
    }
}

interface Block {
    val size: Int
}

interface Array<T> {
    fun get(index: Int): T?
}

interface Dictionary<K, V>

interface Header {
    val checksum: Long

    val size: Int
}

interface Root<T> {
    fun header(): Header?

    fun root(): T?
}

interface SavePrices {
    fun prices(): Array<Price?>?
}

interface Price {
    val open: Double
    val high: Double
    val low: Double
    val last: Double

    fun toBuilder(): PriceMut
}

interface PriceMut : Price {
    override var open: Double
    override var high: Double
    override var low: Double
    override var last: Double
}

class MessageBuffer<T> : Root<T?> {
    override fun header(): Header? {
        return null
    }

    override fun root(): T? {
        return null
    }
}

class PriceImpl private constructor(var address: Long) : PriceMut {
    override var open: Double
        get() = 10.0
        set(value) {}
    override var high: Double
        get() = 15.0
        set(value) {}
    override var low: Double
        get() = 9.1
        set(value) {}
    override var last: Double
        get() = 14.1
        set(value) {}

    override fun toBuilder(): PriceMut {
        return this
    }
}

class HeaderImpl : Header {
    override var size: Int
        get() = 0
        set(value) {}

    override var checksum: Long
        get() = 0
        set(value) {}
}

open class Slice<T>(val buf: Buffer, val baseSize: Int) {
    fun isOutOfBounds(offset: Int, size: Int): Boolean {
        return true
    }
}

data class PriceRecord(
    override var open: Double,
    override var high: Double,
    override var low: Double,
    override var last: Double) : PriceMut {
    override fun toBuilder(): PriceMut {
        TODO("Not yet implemented")
    }
}

class PriceSlice(buf: Buffer, baseSize: Int) : Slice<PriceMut>(buf, baseSize), PriceMut {
    override var open: Double
        get() = buf.bytes.getDoubleLE(0)
        set(value) {}
    override var high: Double
        get() = 15.0
        set(value) {}
    override var low: Double
        get() = 9.1
        set(value) {}
    override var last: Double
        get() = 14.1
        set(value) {}

    override fun toBuilder(): PriceMut {
        return this
    }
}

class PriceSliceBC(buf: Buffer, baseSize: Int) : Slice<PriceMut>(buf, baseSize), PriceMut {
    override var open: Double
        get() = if (buf.isOutOfBounds(baseSize, 8)) 0.0 else buf.bytes.getDoubleLE(0)
        set(value) {
            if (!buf.isOutOfBounds(baseSize, 8))
                buf.bytes.putDoubleLE(0, value)
        }
    override var high: Double
        get() = 15.0
        set(value) {}
    override var low: Double
        get() = 9.1
        set(value) {}
    override var last: Double
        get() = 14.1
        set(value) {}

    override fun toBuilder(): PriceMut {
        return this
    }

    override fun toString(): String {
        return "Price(open=$open, high=$high, low=$low, last=$last)"
    }
}

class Buffer(
    var bytes: BytesMut
) {
    var header: Header = HeaderImpl()
}

fun Buffer.isOutOfBounds(offset: Int, size: Int): Boolean {
    return bytes.size() < offset + size
}

fun readPrice(p: Price) {
    p.toBuilder().apply {
        open = p.open
        high = p.high
        low = p.low
        last = p.last
    }
}

object SomeAction {
    val WALKER = StackWalker.getInstance(mutableSetOf(StackWalker.Option.RETAIN_CLASS_REFERENCE), 1)

    fun doSomething() {
        doSomethingElse()
    }

    fun doSomethingElse() {
        WALKER.forEach {
            println("${it.className}.${it.methodName}:${it.lineNumber}")
        }
        println(WALKER.callerClass)
    }
}

object AbstractStackWalker {
    val cls = Class.forName("java.lang.StackStreamFactory").let { it.nestMembers.find { it.simpleName == "AbstractStackWalker" } }!!
}

object StackFrameTraverser {
    val cls = Class.forName("java.lang.StackStreamFactory").let { it.nestMembers.find { it.simpleName == "StackFrameTraverser" } }!!
    val ctor = cls.declaredConstructors.first().let { it.isAccessible = true; MethodHandles.lookup().unreflectConstructor(it) }

    fun newInstance() = ctor.invoke(StackWalker.getInstance(setOf(StackWalker.Option.RETAIN_CLASS_REFERENCE), 1))


}

class MessageTest2 {
    @Test
    fun testMessage() {
        val buf = Buffer(DirectBytes.allocate(128))
        var x: Double = 0.0
        var price: Price = PriceSliceBC(buf, 128)

        for (y in 0..<10) {
            val time = System.nanoTime()
            for (i in 0..<100_000_000) {
                price = PriceSliceBC(buf, 32)
                x += price.last
                if (time % 2 == 0L) {
                    x += 1.0 + PriceSliceBC(buf, 32).last
                }
            }
        }

        println(price)
        println(x)


    }

    fun handleWalk(stream: Stream<StackWalker.StackFrame>): Int = 0

    @Test
    fun stackWalking() {
        StackWalker.getInstance().forEach { println(it) }
        var cls = Class.forName("java.lang.StackStreamFactory")
        val abstractStackWalker = cls.nestMembers.find { it.simpleName == "AbstractStackWalker" }!!
        val stackFrameTraverser = cls.nestMembers.find { it.simpleName == "StackFrameTraverser" }!!
        val stackFrameTraverserCtor = stackFrameTraverser.declaredConstructors.first()
        stackFrameTraverserCtor.isAccessible = true
        val stackTraverserCtorHandle = MethodHandles.lookup().unreflectConstructor(stackFrameTraverserCtor)
        val stackFrameTraverserInstance = stackTraverserCtorHandle.invoke(StackWalker.getInstance(setOf(StackWalker.Option.DROP_METHOD_INFO), 1), object : Function<Stream<StackWalker.StackFrame>, Int> {
            override fun apply(stream: Stream<StackWalker.StackFrame>): Int {
                return 0
            }
        })
        val initFrameBuffer = stackFrameTraverser.getDeclaredMethod("initFrameBuffer")
        initFrameBuffer.isAccessible = true
        initFrameBuffer.invoke(stackFrameTraverserInstance)
        val frameBufferField = abstractStackWalker.getDeclaredField("frameBuffer")
        frameBufferField.isAccessible = true
        val frameBuffer = frameBufferField.get(stackFrameTraverserInstance)
        val frameBufferClass = frameBuffer.javaClass
        val frameBufferClassMethods = frameBufferClass.superclass.getDeclaredMethod("frames")
        frameBufferClassMethods.isAccessible = true
        val frames = frameBufferClassMethods.invoke(frameBuffer)
//        val framesField = frameBuffer.javaClass.getDeclaredField("stackFrames")
//        framesField.isAccessible = true


//        val stackFrames = framesField.get(frameBuffer)
//        val stackFrameInfoClass = Class.forName("java.lang.StackFrameInfo")
//        val stackFrameInfoClassCtors = stackFrameInfoClass.declaredConstructors
//        val stackFrameInfoClassCtor = stackFrameInfoClass.declaredConstructors.first()
//        stackFrameInfoClassCtor.isAccessible = true
//        val stackFrameInfoClassCtorHandle = MethodHandles.lookup().unreflectConstructor(stackFrameInfoClassCtor)
//        val stackWalker = StackWalker.getInstance()
//        val stackFrames = arrayOf(
//            stackFrameInfoClassCtorHandle.invoke(stackWalker),
//            stackFrameInfoClassCtorHandle.invoke(stackWalker)
//        )


        val callStackWalkMethod = abstractStackWalker.declaredMethods.find { it.name == "callStackWalk" }
//        val callStackWalkMethod = abstractStackWalker.getDeclaredMethod("callStackWalk")
        callStackWalkMethod!!.isAccessible = true
//        callStackWalkMethod.invoke(stackFrameTraverserInstance, 0, 0, null, null, )

        val fetchStackFramesNativeMethod = abstractStackWalker.declaredMethods.find { it.name == "fetchStackFrames" && it.modifiers.and(
            Modifier.NATIVE) == Modifier.NATIVE }
        fetchStackFramesNativeMethod!!.isAccessible = true
        val fetchStackFramesNativeMethodHandle = MethodHandles.lookup().unreflect(fetchStackFramesNativeMethod)

//        val fetchResult = fetchStackFramesNativeMethodHandle.invoke(stackFrameTraverserInstance, 0, 0, 0, 2, 1, stackFrames)

        val fetchStackFramesMethod = abstractStackWalker.getDeclaredMethod("fetchStackFrames")
        fetchStackFramesMethod.isAccessible = true
        val fetchStackFramesHandle = MethodHandles.lookup().unreflect(fetchStackFramesMethod)

        val beginStackWalkMethod = abstractStackWalker.declaredMethods.find { it.name == "beginStackWalk" }!!
        beginStackWalkMethod.isAccessible = true
//        beginStackWalkMethod.invoke(stackFrameTraverserInstance)
//        fetchStackFramesHandle.invoke(stackFrameTraverserInstance)


//        val stackFrames = framesField.get(frameBuffer)

        SomeAction.doSomething()

        var className: String = ""
        var methodName: String = ""
        var lineNumber: Int = 0

        Bench.printHeader()
        Bench.threaded("CallSite", 1, 2, 1_000_000, {t, t2, t3 ->
            SomeAction.WALKER.forEach {  }
//            SomeAction.WALKER.callerClass
        })
        Bench.printFooter()
    }


    /* Header

  -8   u64      hash

   0   i32      size
   4   u8       type
   5   u8       flags
   6   u8       topicOffset
   7   u8       topicSize

   8   i64      topicHash

   16  i64      id

   24  i64      time

   32  u32      sourceIp
   36  u32      sourcePort

   40  i64      sourceId

   48  u32      sourceStarted
   52  u16      headerOffset
   54  u16      headerSize

   56  u8       format
   57  u8       reserved
   58  u16      bodyOffset
   60  i32      bodySize

  */
    class Msg {

    }
}
