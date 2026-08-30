import sys

from setuptools import Extension, setup

extension_compile_args = ["/std:c11", "/experimental:c11atomics"] if sys.platform == "win32" else []

setup(
    name="tarwyn",
    version="0.1.0",
    python_requires=">=3.10",
    packages=["tarwyn"],
    package_data={ "tarwyn": ["py.typed", "*.pyi", "*.dll", "*.dylib", "*.so"] },
    ext_modules=[
        Extension(
            "tarwyn._native",
            sources=["tarwyn/_native.c"],
            extra_compile_args=extension_compile_args,
        ),
    ],
    zip_safe=False,
)
