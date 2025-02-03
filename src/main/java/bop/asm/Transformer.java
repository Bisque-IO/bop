package bop.asm;

import java.io.IOException;
import java.lang.classfile.*;
import java.lang.classfile.attribute.*;
import java.lang.classfile.components.ClassRemapper;
import java.lang.classfile.components.CodeLocalsShifter;
import java.lang.classfile.components.CodeRelabeler;
import java.lang.classfile.instruction.InvokeInstruction;
import java.lang.classfile.instruction.ReturnInstruction;
import java.lang.classfile.instruction.StoreInstruction;
import java.lang.constant.ConstantDescs;
import java.lang.reflect.AccessFlag;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayDeque;
import java.util.Arrays;
import java.util.Map;
import java.util.function.Predicate;
import java.util.stream.Collectors;

public class Transformer {
  static byte[] loadClass(Path className) throws IOException {
    return Files.readAllBytes(className);
  }

  public static void main(String[] args) throws IOException {
    var cf = ClassFile.of();
    var original = loadClass(Path.of("build", "classes", "java", "main", "bop", "Order.class"));
    var classModel = cf.parse(original);
    System.out.println(classModel);

    var after = ClassFile.of().transform(classModel, (builder, element) -> {
      switch (element) {
        case FieldModel f -> {}
        case AccessFlags accessFlags -> {}
        case ClassFileVersion classFileVersion -> {}
        case CustomAttribute customAttribute -> {}
        case Interfaces interfaces -> {}
        case MethodModel methodModel -> {
          if (methodModel.methodName().equals("<init>")) {
            System.out.println();
            System.out.println(methodModel);
            methodModel.code();
            methodModel
                .code()
                .ifPresent(c -> c.forEach(codeElement -> System.out.println(codeElement)));
            System.out.println();
            //         methodModel.elements().forEach(e ->
            // System.out.println(e));
          }
        }
        case Superclass superclass -> {}
        case CompilationIDAttribute compilationIDAttribute -> {}
        case DeprecatedAttribute deprecatedAttribute -> {}
        case EnclosingMethodAttribute enclosingMethodAttribute -> {}
        case InnerClassesAttribute innerClassesAttribute -> {}
        case ModuleAttribute moduleAttribute -> {}
        case ModuleHashesAttribute moduleHashesAttribute -> {}
        case ModuleMainClassAttribute moduleMainClassAttribute -> {}
        case ModulePackagesAttribute modulePackagesAttribute -> {}
        case ModuleResolutionAttribute moduleResolutionAttribute -> {}
        case ModuleTargetAttribute moduleTargetAttribute -> {}
        case NestHostAttribute nestHostAttribute -> {}
        case NestMembersAttribute nestMembersAttribute -> {}
        case PermittedSubclassesAttribute permittedSubclassesAttribute -> {}
        case RecordAttribute recordAttribute -> {}
        case RuntimeInvisibleAnnotationsAttribute runtimeInvisibleAnnotationsAttribute -> {}
        case RuntimeInvisibleTypeAnnotationsAttribute runtimeInvisibleTypeAnnotationsAttribute -> {}
        case RuntimeVisibleAnnotationsAttribute runtimeVisibleAnnotationsAttribute -> {}
        case RuntimeVisibleTypeAnnotationsAttribute runtimeVisibleTypeAnnotationsAttribute -> {}
        case SignatureAttribute signatureAttribute -> {}
        case SourceDebugExtensionAttribute sourceDebugExtensionAttribute -> {}
        case SourceFileAttribute sourceFileAttribute -> {}
        case SourceIDAttribute sourceIDAttribute -> {}
        case SyntheticAttribute syntheticAttribute -> {}
        case UnknownAttribute unknownAttribute -> {}
      }
      System.out.println(element);
      builder.with(element);
    });
    System.out.println(original);
    System.out.println(after);
    Files.write(Path.of("Order.class"), after);
    System.out.println("before: " + original.length + "   after: " + after.length + "   "
        + Arrays.equals(original, after));
  }

  static byte[] classInstrumentation(
      ClassModel target,
      ClassModel instrumentor,
      Predicate<MethodModel> instrumentedMethodsFilter) {
    var instrumentorCodeMap = instrumentor.methods().stream()
        .filter(instrumentedMethodsFilter)
        .collect(Collectors.toMap(
            mm -> mm.methodName().stringValue() + mm.methodType().stringValue(),
            mm -> mm.code().orElseThrow()));
    var targetFieldNames =
        target.fields().stream().map(f -> f.fieldName().stringValue()).collect(Collectors.toSet());
    var targetMethods = target.methods().stream()
        .map(m -> m.methodName().stringValue() + m.methodType().stringValue())
        .collect(Collectors.toSet());
    var instrumentorClassRemapper = ClassRemapper.of(
        Map.of(instrumentor.thisClass().asSymbol(), target.thisClass().asSymbol()));
    return ClassFile.of()
        .transform(
            target,
            ClassTransform.transformingMethods(instrumentedMethodsFilter, (mb, me) -> {
                  if (me instanceof CodeModel targetCodeModel) {
                    var mm = targetCodeModel.parent().get();
                    // instrumented methods code is taken from
                    // instrumentor
                    mb.transformCode(
                        instrumentorCodeMap.get(
                            mm.methodName().stringValue() + mm.methodType().stringValue()),
                        // all references to the instrumentor class
                        // are remapped to target class
                        instrumentorClassRemapper
                            .asCodeTransform()
                            .andThen((codeBuilder, instrumentorCodeElement) -> {
                              // all invocations of
                              // target methods from
                              // instrumentor are
                              // inlined
                              if (instrumentorCodeElement instanceof InvokeInstruction inv
                                  && target
                                      .thisClass()
                                      .asInternalName()
                                      .equals(inv.owner().asInternalName())
                                  && mm.methodName()
                                      .stringValue()
                                      .equals(inv.name().stringValue())
                                  && mm.methodType()
                                      .stringValue()
                                      .equals(inv.type().stringValue())) {

                                // store stacked
                                // method parameters
                                // into locals
                                var storeStack = new ArrayDeque<StoreInstruction>();
                                int slot = 0;
                                if (!mm.flags().has(AccessFlag.STATIC))
                                  storeStack.push(
                                      StoreInstruction.of(TypeKind.ReferenceType, slot++));
                                for (var pt : mm.methodTypeSymbol().parameterList()) {
                                  var tk = TypeKind.from(pt);
                                  storeStack.push(StoreInstruction.of(tk, slot));
                                  slot += tk.slotSize();
                                }
                                storeStack.forEach(codeBuilder::with);

                                // inlined target
                                // locals must be
                                // shifted based on
                                // the
                                // actual
                                // instrumentor
                                // locals
                                codeBuilder.block(
                                    inlinedBlockBuilder -> inlinedBlockBuilder.transform(
                                        targetCodeModel,
                                        CodeLocalsShifter.of(mm.flags(), mm.methodTypeSymbol())
                                            .andThen(CodeRelabeler.of())
                                            .andThen(
                                                (innerBuilder, shiftedTargetCode) -> {
                                                  // returns must be
                                                  // replaced with
                                                  // jump
                                                  // to the end of the
                                                  // inlined method
                                                  if (shiftedTargetCode
                                                      instanceof ReturnInstruction)
                                                    innerBuilder.goto_(
                                                        inlinedBlockBuilder.breakLabel());
                                                  else innerBuilder.with(shiftedTargetCode);
                                                })));
                              } else codeBuilder.with(instrumentorCodeElement);
                            }));
                  } else mb.with(me);
                })
                .andThen(ClassTransform.endHandler(clb ->
                    // remaining instrumentor fields and methods
                    // are injected at the end
                    clb.transform(
                        instrumentor,
                        ClassTransform.dropping(cle -> !(cle instanceof FieldModel fm
                                    && !targetFieldNames.contains(fm.fieldName().stringValue()))
                                && !(cle instanceof MethodModel mm
                                    && !ConstantDescs.INIT_NAME.equals(
                                        mm.methodName().stringValue())
                                    && !targetMethods.contains(mm.methodName().stringValue()
                                        + mm.methodType().stringValue())))
                            // and instrumentor class
                            // references remapped to
                            // target class
                            .andThen(instrumentorClassRemapper)))));
  }
}
