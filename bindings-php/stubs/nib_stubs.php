<?php

// IDE stub only — never require/include this file. The real Nib class is
// provided at runtime by the compiled bindings-php extension (see CLAUDE.md).

class Nib
{
    public function __construct() {}

    public function parse(string $source): void {}

    public function run(): void {}

    public function registerFunc(string $name, callable $callback): void {}

    public function registerVar(string $name, mixed $value): void {}

    public function include(string $sourcename): void {}

    public function disableKeywords(array $keywords): void {}
}
