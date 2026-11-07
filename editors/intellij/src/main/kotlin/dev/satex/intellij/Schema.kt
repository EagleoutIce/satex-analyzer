package dev.satex.intellij

import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.intellij.openapi.vfs.VfsUtil
import com.jetbrains.jsonSchema.extension.JsonSchemaFileProvider
import com.jetbrains.jsonSchema.extension.JsonSchemaProviderFactory
import com.jetbrains.jsonSchema.extension.SchemaType

/** Maps satex.yaml to the bundled satex.schema.json. */
class SatexSchemaFactory : JsonSchemaProviderFactory {
    override fun getProviders(project: Project): List<JsonSchemaFileProvider> = listOf(object : JsonSchemaFileProvider {
        override fun isAvailable(file: VirtualFile) = file.name == "satex.yaml" || file.name == "satex.yml"
        override fun getName() = "satex.yaml"
        override fun getSchemaFile(): VirtualFile? =
            SatexSchemaFactory::class.java.getResource("/schemas/satex.schema.json")?.let { VfsUtil.findFileByURL(it) }
        override fun getSchemaType() = SchemaType.embeddedSchema
    })
}
