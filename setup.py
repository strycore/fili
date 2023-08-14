from setuptools import setup

setup(
    name='fili',
    version='0.0.1',
    py_modules=['fili'],
    install_requires=[
        'Click',
    ],
    entry_points={
        'console_scripts': [
            'fili = fili:main',
        ],
    },
)
