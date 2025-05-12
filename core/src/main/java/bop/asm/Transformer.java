package bop.asm;

import java.io.IOException;
import java.lang.classfile.*;
import java.lang.classfile.attribute.*;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;

public class Transformer {
  static byte[] loadClass(Path className) throws IOException {
    return Files.readAllBytes(className);
  }

  public static void main(String[] args) throws IOException {
    var cf = ClassFile.of();
    var original =
        loadClass(Path.of("build", "classes", "java", "main", "bop", "kernel", "Epoch.class"));
    var classModel = cf.parse(original);
    System.out.println(classModel);

    var after = ClassFile.of().transformClass(classModel, (builder, element) -> {
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
}
